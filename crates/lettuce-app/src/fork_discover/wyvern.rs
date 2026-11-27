//! Wyvern (wyvern.chat). The public REST API at app.wyvern.chat returns full
//! card definitions in search results and card pages; linked lorebooks come
//! from `/api/lorebooks/<id>` in a V2-like entry format. No account needed.

use lettuce_contracts::{self as dto, ApiError, DiscoverKind as Kind, DiscoverSource};
use serde_json::{Value, json};

use super::card_png::write_card_png;
use super::common::{
    HttpRequest, array_len, clip, comma_list, count, file_name, html_to_text, json, malformed, next_page, opt_text,
    page_of, strings, text, unsupported,
};
use super::{Downloaded, Fetch};

const API: &str = "https://app.wyvern.chat/api";
const SITE: &str = "https://app.wyvern.chat";
const PAGE_SIZE: u32 = 24;
const NAME: &str = "Wyvern";
/// A card links at most a few lorebooks; more than this is not fetched.
const MAX_LOREBOOKS: usize = 5;

fn characters_only(kind: Kind) -> Result<(), ApiError> {
    if kind == Kind::Character { Ok(()) } else { Err(unsupported("Wyvern browsing covers characters only for now")) }
}

fn checked_id(id: &str) -> Result<&str, ApiError> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        Ok(id)
    } else {
        Err(unsupported("not a Wyvern id"))
    }
}

pub(crate) fn search(request: &dto::DiscoverSearchRequest) -> Result<HttpRequest, ApiError> {
    characters_only(request.kind)?;
    let page = page_of(request.cursor.as_deref());
    let tags = comma_list(&request.tags);
    Ok(HttpRequest::get(format!("{API}/characters/public"))
        .param_if(!request.query.trim().is_empty(), "query", request.query.trim())
        .param("page", page)
        .param("limit", PAGE_SIZE)
        .param("sort", request.sort.as_deref().filter(|s| !s.is_empty()).unwrap_or("total_likes"))
        .param("order", "DESC")
        .param_if(!tags.is_empty(), "tags", tags)
        .param_if(!request.nsfw, "rating", "none")
        .param_if(request.nsfw, "show_nsfw", "true"))
}

fn card(node: &Value) -> Option<dto::DiscoverCard> {
    let id = opt_text(node, "id").or_else(|| opt_text(node, "_id"))?;
    let creator = node.get("creator").cloned().unwrap_or(Value::Null);
    let stats = node.get("entity_statistics").cloned().unwrap_or(Value::Null);
    let record = node.get("statistics_record").cloned().unwrap_or(Value::Null);
    let rating = text(node, "rating");
    let tagline = html_to_text(&opt_text(node, "tagline").or_else(|| opt_text(node, "creator_notes")).unwrap_or_default());
    Some(dto::DiscoverCard {
        source: DiscoverSource::Wyvern,
        kind: Kind::Character,
        name: opt_text(node, "name").or_else(|| opt_text(node, "chat_name")).unwrap_or_else(|| id.clone()),
        tagline: clip(&tagline, 220),
        avatar_url: opt_text(node, "avatar"),
        creator: opt_text(&creator, "displayName").or_else(|| opt_text(&creator, "vanityUrl")),
        tags: strings(node, "tags"),
        nsfw: rating == "mature" || rating == "explicit",
        tokens: count(node, "token_count"),
        stats: dto::DiscoverStats {
            likes: count(&stats, "total_likes").or_else(|| count(&record, "likes")).or_else(|| count(node, "likes")),
            views: count(&stats, "total_views").or_else(|| count(&record, "views")),
            messages: count(&stats, "total_messages").or_else(|| count(&record, "messages")),
            chats: count(&stats, "total_chats").or_else(|| count(&record, "chats")),
            ..Default::default()
        },
        page_url: Some(format!("{SITE}/characters/{id}")),
        id,
    })
}

pub(crate) fn parse_search(body: &[u8], request: &dto::DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    let value = json(body, NAME, "search page")?;
    let nodes = value.get("characters").and_then(Value::as_array).ok_or_else(|| malformed(NAME, "search page"))?;
    let page = page_of(request.cursor.as_deref());
    let total = count(&value, "total");
    let has_more = !nodes.is_empty() && total.map_or(nodes.len() as u32 >= PAGE_SIZE, |total| page * PAGE_SIZE < total);
    Ok(dto::DiscoverSearchPage { items: nodes.iter().filter_map(card).collect(), next_cursor: next_page(page, has_more), total })
}

pub(crate) fn detail(kind: Kind, id: &str) -> Result<HttpRequest, ApiError> {
    characters_only(kind)?;
    Ok(HttpRequest::get(format!("{API}/characters/{}", checked_id(id)?)))
}

fn lorebook_ids(node: &Value) -> Vec<String> {
    node.get("lorebooks")
        .and_then(Value::as_array)
        .map(|books| {
            books
                .iter()
                .filter_map(|book| book.as_str().map(str::to_owned).or_else(|| opt_text(book, "id")).or_else(|| opt_text(book, "_id")))
                .filter(|id| checked_id(id).is_ok())
                .take(MAX_LOREBOOKS)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_detail(body: &[u8]) -> Result<dto::DiscoverDetail, ApiError> {
    let node = json(body, NAME, "card")?;
    let card = card(&node).ok_or_else(|| malformed(NAME, "card"))?;
    let description = [text(&node, "creator_notes"), text(&node, "shared_info"), text(&node, "tagline")]
        .into_iter()
        .map(|text| html_to_text(&text))
        .find(|text| !text.is_empty())
        .unwrap_or_default();
    Ok(dto::DiscoverDetail {
        card,
        description,
        greeting: opt_text(&node, "first_mes"),
        alternate_greetings: array_len(&node, "alternate_greetings"),
        has_lorebook: Some(!lorebook_ids(&node).is_empty()),
        created_at: opt_text(&node, "created_at"),
        updated_at: opt_text(&node, "updated_at"),
        import_blocker: None,
    })
}

/// Wyvern lorebooks merged into one V2 `character_book`.
pub(crate) fn character_book(books: &[Value]) -> Option<Value> {
    let entries: Vec<Value> = books
        .iter()
        .flat_map(|book| book.get("entries").and_then(Value::as_array).cloned().unwrap_or_default())
        .enumerate()
        .map(|(index, entry)| {
            let secondary = entry.get("secondary_keys").cloned().unwrap_or_else(|| json!([]));
            json!({
                "id": index,
                "keys": entry.get("keys").cloned().unwrap_or_else(|| json!([])),
                "secondary_keys": secondary,
                "selective": secondary.as_array().is_some_and(|keys| !keys.is_empty()),
                "content": text(&entry, "content"),
                "name": text(&entry, "name"),
                "comment": opt_text(&entry, "comment").unwrap_or_else(|| text(&entry, "name")),
                "enabled": entry.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                "constant": entry.get("constant").and_then(Value::as_bool).unwrap_or(false),
                "case_sensitive": entry.get("case_sensitive").and_then(Value::as_bool).unwrap_or(false),
                "insertion_order": entry.get("insertion_order").and_then(Value::as_i64).unwrap_or(100),
                "priority": entry.get("priority").and_then(Value::as_i64).unwrap_or(0),
                "position": if text(&entry, "position") == "after_char" { "after_char" } else { "before_char" },
                "extensions": entry.get("extensions").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})),
            })
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    let first = books.first().cloned().unwrap_or(Value::Null);
    Some(json!({
        "name": text(&first, "name"),
        "description": text(&first, "description"),
        "scan_depth": first.get("scan_depth").and_then(Value::as_i64).filter(|d| *d > 0).unwrap_or(4),
        "token_budget": first.get("token_budget").and_then(Value::as_i64).unwrap_or(2048),
        "recursive_scanning": first.get("recursive_scanning").and_then(Value::as_bool).unwrap_or(false),
        "extensions": {},
        "entries": entries,
    }))
}

/// A Tavern V2 card from a Wyvern character.
pub(crate) fn card_v2(node: &Value, books: &[Value]) -> Value {
    let creator = node.get("creator").cloned().unwrap_or(Value::Null);
    let notes = [text(node, "creator_notes"), text(node, "shared_info")].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    let mut description = text(node, "description");
    for extra in [text(node, "visual_description"), text(node, "character_note")] {
        if !extra.is_empty() {
            description = format!("{description}\n\n{extra}");
        }
    }
    let mut data = json!({
        "name": opt_text(node, "chat_name").or_else(|| opt_text(node, "name")).unwrap_or_default(),
        "description": description.trim(),
        "personality": text(node, "personality"),
        "scenario": text(node, "scenario"),
        "first_mes": text(node, "first_mes"),
        "mes_example": text(node, "mes_example"),
        "creator_notes": notes,
        "system_prompt": text(node, "pre_history_instructions"),
        "post_history_instructions": text(node, "post_history_instructions"),
        "alternate_greetings": strings(node, "alternate_greetings"),
        "tags": strings(node, "tags"),
        "creator": opt_text(&creator, "displayName").unwrap_or_default(),
        "character_version": "",
        "extensions": {},
    });
    if let (Some(book), Some(fields)) = (character_book(books), data.as_object_mut()) {
        fields.insert("character_book".to_owned(), book);
    }
    json!({ "spec": "chara_card_v2", "spec_version": "2.0", "data": data })
}

pub(crate) async fn download(fetch: &impl Fetch, kind: Kind, id: &str) -> Result<Downloaded, ApiError> {
    let node = json(&fetch.fetch(NAME, detail(kind, id)?).await?.body, NAME, "card")?;
    let mut books = Vec::new();
    for book in lorebook_ids(&node) {
        if let Ok(file) = fetch.fetch(NAME, HttpRequest::get(format!("{API}/lorebooks/{book}"))).await
            && let Ok(value) = serde_json::from_slice::<Value>(&file.body)
        {
            books.push(value);
        }
    }
    let card = card_v2(&node, &books);
    let image = match opt_text(&node, "avatar") {
        Some(url) => fetch.fetch(NAME, HttpRequest::file(url)).await.ok().map(|file| file.body),
        None => None,
    };
    let name = text(&card["data"], "name");
    Ok(Downloaded { bytes: write_card_png(image.as_deref(), &card)?, file_name: file_name(&name, "png") })
}
