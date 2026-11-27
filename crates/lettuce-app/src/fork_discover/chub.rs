//! Chub (chub.ai, formerly characterhub.org). Search and details come from
//! the public gateway API; card images from the avatars CDN. No account is
//! needed for public cards. Chub blocks some countries and data-center IPs
//! at Cloudflare, so the gateway can answer 403 where the CDN still works.

use lettuce_contracts::{self as dto, ApiError, DiscoverKind as Kind, DiscoverSource};
use serde_json::{Value, json};

use super::card_png::write_card_png;
use super::common::{public_https, 
    HttpRequest, array_len, clip, comma_list, count, file_name, html_to_text, json, malformed, next_page, opt_text,
    page_of, strings, text, unsupported,
};
use super::{Downloaded, Fetch};

const GATEWAY: &str = "https://gateway.chub.ai";
const CDN: &str = "https://avatars.charhub.io/avatars";
const SITE: &str = "https://chub.ai";
const PAGE_SIZE: u32 = 24;
const NAME: &str = "Chub";

fn namespace(kind: Kind) -> Result<&'static str, ApiError> {
    match kind {
        Kind::Character => Ok("characters"),
        Kind::Lorebook => Ok("lorebooks"),
        Kind::Preset => Err(unsupported("Chub has no prompt presets")),
    }
}

/// Chub paths are `creator/slug`; anything else is refused before it reaches a URL.
fn checked_path(id: &str) -> Result<&str, ApiError> {
    let valid = id.split('/').count() == 2
        && id.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && part.chars().all(|c| c.is_alphanumeric() || "-_.~".contains(c)));
    if valid { Ok(id) } else { Err(unsupported("not a Chub card path")) }
}

pub(crate) fn search(request: &dto::DiscoverSearchRequest) -> Result<HttpRequest, ApiError> {
    let namespace = namespace(request.kind)?;
    let page = page_of(request.cursor.as_deref());
    let tags = comma_list(&request.tags);
    let excluded = comma_list(&request.exclude_tags);
    Ok(HttpRequest::get(format!("{GATEWAY}/search"))
        .param("search", request.query.trim())
        .param("namespace", namespace)
        .param("first", PAGE_SIZE)
        .param("page", page)
        .param("sort", request.sort.as_deref().filter(|s| !s.is_empty()).unwrap_or("default"))
        .param("asc", "false")
        .param("nsfw", request.nsfw)
        .param("nsfl", request.nsfw && request.nsfl)
        .param("include_forks", "true")
        .param("chub", "true")
        .param_if(!tags.is_empty(), "topics", tags)
        .param_if(!excluded.is_empty(), "excludetopics", excluded))
}

fn card(node: &Value, kind: Kind) -> Option<dto::DiscoverCard> {
    // Lorebook paths come back as `lorebooks/creator/slug`.
    let full = text(node, "fullPath");
    let path = full.strip_prefix("lorebooks/").unwrap_or(&full).to_owned();
    if path.is_empty() {
        return None;
    }
    let tags = strings(node, "topics");
    let segment = if kind == Kind::Lorebook { "lorebooks/" } else { "" };
    let tagline = opt_text(node, "tagline").unwrap_or_else(|| clip(&html_to_text(&text(node, "description")), 200));
    Some(dto::DiscoverCard {
        source: DiscoverSource::Chub,
        kind,
        id: path.clone(),
        name: opt_text(node, "name").unwrap_or_else(|| path.clone()),
        tagline,
        avatar_url: opt_text(node, "avatar_url").or_else(|| Some(format!("{CDN}/{segment}{path}/avatar.webp"))),
        creator: path.split('/').next().map(str::to_owned),
        nsfw: node.get("nsfw_image").and_then(Value::as_bool).unwrap_or(false)
            || tags.iter().any(|tag| tag.eq_ignore_ascii_case("nsfw")),
        tags,
        tokens: count(node, "nTokens"),
        stats: dto::DiscoverStats {
            stars: count(node, "starCount"),
            chats: count(node, "nChats"),
            messages: count(node, "nMessages"),
            likes: count(node, "n_favorites"),
            rating: node.get("rating").and_then(Value::as_f64).filter(|r| *r > 0.0).map(|r| r as f32),
            ..Default::default()
        },
        page_url: Some(format!("{SITE}/{}/{path}", if kind == Kind::Lorebook { "lorebooks" } else { "characters" })),
    })
}

pub(crate) fn parse_search(body: &[u8], request: &dto::DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    let value = json(body, NAME, "search page")?;
    let data = value.get("data").unwrap_or(&value);
    let nodes = data.get("nodes").and_then(Value::as_array).ok_or_else(|| malformed(NAME, "search page"))?;
    let items: Vec<_> = nodes.iter().filter_map(|node| card(node, request.kind)).collect();
    let page = page_of(request.cursor.as_deref());
    let total = count(data, "count");
    let has_more = match total {
        Some(total) => page * PAGE_SIZE < total,
        None => u32::try_from(nodes.len()).unwrap_or(0) >= PAGE_SIZE,
    } && !nodes.is_empty();
    Ok(dto::DiscoverSearchPage { items, next_cursor: next_page(page, has_more), total })
}

fn api_path(kind: Kind) -> &'static str {
    if kind == Kind::Lorebook { "lorebooks" } else { "characters" }
}

pub(crate) fn detail(kind: Kind, id: &str) -> Result<HttpRequest, ApiError> {
    namespace(kind)?;
    let path = checked_path(id)?;
    Ok(HttpRequest::get(format!("{GATEWAY}/api/{}/{path}", api_path(kind))).param("full", "true"))
}

fn definition(node: &Value) -> Value {
    match node.get("definition") {
        Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or(Value::Null),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn has_entries(book: &Value) -> bool {
    match book.get("entries") {
        Some(Value::Array(entries)) => !entries.is_empty(),
        Some(Value::Object(entries)) => !entries.is_empty(),
        _ => false,
    }
}

pub(crate) fn parse_detail(body: &[u8], kind: Kind) -> Result<dto::DiscoverDetail, ApiError> {
    let value = json(body, NAME, "card")?;
    let node = value.get("node").unwrap_or(&value);
    let card = card(node, kind).ok_or_else(|| malformed(NAME, "card"))?;
    let def = definition(node);
    let description = [text(node, "description"), text(&def, "description"), text(node, "tagline")]
        .into_iter()
        .map(|text| html_to_text(&text))
        .find(|text| !text.is_empty())
        .unwrap_or_default();
    let linked = node
        .get("related_lorebooks")
        .and_then(Value::as_array)
        .is_some_and(|ids| ids.iter().any(|id| id.as_i64().is_some_and(|id| id > 0)));
    Ok(dto::DiscoverDetail {
        card,
        description,
        greeting: opt_text(&def, "first_message").or_else(|| opt_text(&def, "first_mes")),
        alternate_greetings: array_len(&def, "alternate_greetings"),
        has_lorebook: (kind == Kind::Character).then(|| def.get("embedded_lorebook").is_some_and(has_entries) || linked),
        created_at: opt_text(node, "createdAt"),
        updated_at: opt_text(node, "lastActivityAt"),
        import_blocker: None,
    })
}

/// A Tavern V2 card from the gateway's full definition, using the same field
/// mapping as SillyTavern's own Chub importer (Chub's `personality` is the
/// card description, `tavern_personality` the personality, `description` the
/// creator notes).
pub(crate) fn card_v2(node: &Value) -> Value {
    let def = definition(node);
    let path = text(node, "fullPath");
    let book = def.get("embedded_lorebook").filter(|book| has_entries(book)).cloned();
    let mut data = json!({
        "name": opt_text(&def, "name").unwrap_or_else(|| text(node, "name")),
        "description": text(&def, "personality"),
        "personality": text(&def, "tavern_personality"),
        "scenario": text(&def, "scenario"),
        "first_mes": opt_text(&def, "first_message").unwrap_or_else(|| text(&def, "first_mes")),
        "mes_example": opt_text(&def, "example_dialogs").unwrap_or_else(|| text(&def, "mes_example")),
        "creator_notes": text(&def, "description"),
        "system_prompt": text(&def, "system_prompt"),
        "post_history_instructions": text(&def, "post_history_instructions"),
        "alternate_greetings": def.get("alternate_greetings").cloned().unwrap_or_else(|| json!([])),
        "tags": node.get("topics").cloned().unwrap_or_else(|| json!([])),
        "creator": path.split('/').next().unwrap_or_default(),
        "character_version": "",
        "extensions": def.get("extensions").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})),
    });
    if let (Some(book), Some(fields)) = (book, data.as_object_mut()) {
        fields.insert("character_book".to_owned(), book);
    }
    json!({ "spec": "chara_card_v2", "spec_version": "2.0", "data": data })
}

pub(crate) async fn download(fetch: &impl Fetch, kind: Kind, id: &str) -> Result<Downloaded, ApiError> {
    let path = checked_path(id)?.to_owned();
    let slug = path.rsplit('/').next().unwrap_or("card").to_owned();
    match kind {
        Kind::Preset => Err(unsupported("Chub has no prompt presets")),
        Kind::Lorebook => {
            let meta = json(&fetch.fetch(NAME, detail(kind, &path)?).await?.body, NAME, "lorebook")?;
            let project = meta.get("node").and_then(|node| node.get("id")).and_then(Value::as_i64).ok_or_else(|| malformed(NAME, "lorebook"))?;
            let raw = fetch
                .fetch(NAME, HttpRequest::file(format!("{GATEWAY}/api/v4/projects/{project}/repository/files/raw%252Fsillytavern_raw.json/raw")).param("ref", "main").param("response_type", "blob"))
                .await?;
            json(&raw.body, NAME, "lorebook file")?;
            Ok(Downloaded { bytes: raw.body, file_name: file_name(&slug, "json") })
        }
        Kind::Character => {
            // Fresh definitions from the API first: the CDN PNG can carry an
            // older card than the page shows (SillyTavern issue #4312).
            let fresh = async {
                let value = json(&fetch.fetch(NAME, detail(kind, &path)?).await?.body, NAME, "card")?;
                let node = value.get("node").cloned().unwrap_or(value);
                let card = card_v2(&node);
                let image_url = opt_text(&node, "max_res_url").or_else(|| opt_text(&node, "avatar_url")).and_then(|url| public_https(&url));
                let image = match image_url {
                    Some(url) => fetch.fetch(NAME, HttpRequest::file(url)).await.ok().map(|file| file.body),
                    None => None,
                };
                let name = text(&card["data"], "name");
                Ok::<_, ApiError>(Downloaded { bytes: write_card_png(image.as_deref(), &card)?, file_name: file_name(&name, "png") })
            }
            .await;
            match fresh {
                Ok(file) => Ok(file),
                Err(api_error) => {
                    let cdn = fetch.fetch(NAME, HttpRequest::file(format!("{CDN}/{path}/chara_card_v2.png"))).await.map_err(|_| api_error)?;
                    Ok(Downloaded { bytes: cdn.body, file_name: file_name(&slug, "png") })
                }
            }
        }
    }
}
