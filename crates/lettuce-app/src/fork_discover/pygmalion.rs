//! Pygmalion (pygmalion.chat). Public Connect-RPC endpoints on
//! server.pygmalion.chat search and describe cards; `/api/export/character/<id>/v2`
//! returns a V2 card. Anonymous requests only see SFW cards: the
//! `includeSensitive` flag needs a signed-in account.

use lettuce_contracts::{self as dto, ApiError, DiscoverKind as Kind, DiscoverSource};
use serde_json::{Value, json};

use super::card_png::write_card_png;
use super::common::{
    HttpRequest, clip, count, file_name, json, malformed, next_page, opt_text, page_of, strings, text, unsupported,
};
use super::{Downloaded, Fetch};

const SERVER: &str = "https://server.pygmalion.chat";
const SERVICE: &str = "https://server.pygmalion.chat/galatea.v1.PublicCharacterService";
const SITE: &str = "https://pygmalion.chat";
const PAGE_SIZE: u32 = 24;
const NAME: &str = "Pygmalion";

fn characters_only(kind: Kind) -> Result<(), ApiError> {
    if kind == Kind::Character { Ok(()) } else { Err(unsupported("Pygmalion hosts characters only")) }
}

fn checked_id(id: &str) -> Result<&str, ApiError> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        Ok(id)
    } else {
        Err(unsupported("not a Pygmalion character id"))
    }
}

fn rpc(method: &str, body: &Value) -> HttpRequest {
    HttpRequest::post(format!("{SERVICE}/{method}"), body).header("connect-protocol-version", "1")
}

pub(crate) fn search(request: &dto::DiscoverSearchRequest) -> Result<HttpRequest, ApiError> {
    characters_only(request.kind)?;
    let page = page_of(request.cursor.as_deref());
    let mut body = json!({
        "orderBy": request.sort.as_deref().filter(|s| !s.is_empty()).unwrap_or("downloads"),
        "orderDescending": true,
        "includeSensitive": request.nsfw,
        "pageSize": PAGE_SIZE,
        "pageNumber": page - 1,
    });
    if !request.query.trim().is_empty() {
        body["query"] = json!(request.query.trim());
    }
    if !request.tags.is_empty() {
        body["tagsNamesInclude"] = json!(request.tags);
    }
    if !request.exclude_tags.is_empty() {
        body["tagsNamesExclude"] = json!(request.exclude_tags);
    }
    Ok(rpc("CharacterSearch", &body))
}

fn card(node: &Value) -> Option<dto::DiscoverCard> {
    let id = text(node, "id");
    if id.is_empty() {
        return None;
    }
    let owner = node.get("owner").cloned().unwrap_or(Value::Null);
    Some(dto::DiscoverCard {
        source: DiscoverSource::Pygmalion,
        kind: Kind::Character,
        name: opt_text(node, "displayName").unwrap_or_else(|| id.clone()),
        tagline: clip(&text(node, "description"), 220),
        avatar_url: opt_text(node, "avatarUrl"),
        creator: opt_text(&owner, "displayName").or_else(|| opt_text(&owner, "username")),
        tags: strings(node, "tags"),
        nsfw: node.get("isSensitive").and_then(Value::as_bool).unwrap_or(false),
        tokens: count(node, "personalityTokenCount"),
        stats: dto::DiscoverStats {
            stars: count(node, "stars"),
            downloads: count(node, "downloads"),
            chats: count(node, "chatCount"),
            views: count(node, "views"),
            ..Default::default()
        },
        page_url: Some(format!("{SITE}/character/{id}")),
        id,
    })
}

pub(crate) fn parse_search(body: &[u8], request: &dto::DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    let value = json(body, NAME, "search page")?;
    let nodes = value.get("characters").and_then(Value::as_array).cloned().unwrap_or_default();
    if value.get("characters").is_none() && value.get("totalItems").is_none() {
        return Err(malformed(NAME, "search page"));
    }
    let page = page_of(request.cursor.as_deref());
    let total = count(&value, "totalItems");
    let has_more = !nodes.is_empty() && total.map_or(nodes.len() as u32 >= PAGE_SIZE, |total| page * PAGE_SIZE < total);
    Ok(dto::DiscoverSearchPage { items: nodes.iter().filter_map(card).collect(), next_cursor: next_page(page, has_more), total })
}

pub(crate) fn detail(kind: Kind, id: &str) -> Result<HttpRequest, ApiError> {
    characters_only(kind)?;
    Ok(rpc("Character", &json!({ "characterMetaId": checked_id(id)?, "characterVersionId": "" })))
}

fn epoch(value: &Value, key: &str) -> Option<String> {
    let seconds = count(value, key)?;
    chrono::DateTime::from_timestamp(i64::from(seconds), 0).map(|time| time.to_rfc3339())
}

pub(crate) fn parse_detail(body: &[u8]) -> Result<dto::DiscoverDetail, ApiError> {
    let value = json(body, NAME, "card")?;
    let node = value.get("character").unwrap_or(&value);
    let mut card = card(node).ok_or_else(|| malformed(NAME, "card"))?;
    let personality = node.get("personality").cloned().unwrap_or(Value::Null);
    let description = text(node, "description");
    card.tagline = clip(&description, 220);
    Ok(dto::DiscoverDetail {
        card,
        description: [description, text(&personality, "characterNotes")].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n"),
        greeting: opt_text(&personality, "greeting"),
        alternate_greetings: super::common::array_len(&personality, "alternateGreetings"),
        has_lorebook: None,
        created_at: epoch(node, "createdAt"),
        updated_at: epoch(node, "updatedAt"),
        import_blocker: None,
    })
}

pub(crate) async fn download(fetch: &impl Fetch, kind: Kind, id: &str) -> Result<Downloaded, ApiError> {
    characters_only(kind)?;
    let id = checked_id(id)?;
    let exported = json(&fetch.fetch(NAME, HttpRequest::get(format!("{SERVER}/api/export/character/{id}/v2"))).await?.body, NAME, "export")?;
    let card = exported.get("character").cloned().ok_or_else(|| malformed(NAME, "export"))?;
    if card.get("data").is_none() {
        return Err(malformed(NAME, "export"));
    }
    let image = match opt_text(&card["data"], "avatar") {
        Some(url) => fetch.fetch(NAME, HttpRequest::file(url)).await.ok().map(|file| file.body),
        None => None,
    };
    let name = text(&card["data"], "name");
    Ok(Downloaded { bytes: write_card_png(image.as_deref(), &card)?, file_name: file_name(&name, "png") })
}
