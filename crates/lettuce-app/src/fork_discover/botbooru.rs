//! Botbooru (botbooru.com), a booru-style card archive. Its site JSON
//! (`/posts/`, `/post/<id>`) needs no account; `/download/png/<id>` returns
//! the card PNG. Search has a per-IP quota and answers 429 when it is spent.

use lettuce_contracts::{self as dto, ApiError, DiscoverKind as Kind, DiscoverSource};
use serde_json::Value;

use super::common::{
    HttpRequest, array_len, clip, count, file_name, html_to_text, json, malformed, next_page, opt_text, page_of, text,
    unsupported,
};
use super::{Downloaded, Fetch};

const SITE: &str = "https://botbooru.com";
const PAGE_SIZE: u32 = 24;
const NAME: &str = "Botbooru";

fn characters_only(kind: Kind) -> Result<(), ApiError> {
    if kind == Kind::Character { Ok(()) } else { Err(unsupported("Botbooru browsing covers characters only for now")) }
}

fn checked_id(id: &str) -> Result<&str, ApiError> {
    if !id.is_empty() && id.len() <= 20 && id.chars().all(|c| c.is_ascii_digit()) { Ok(id) } else { Err(unsupported("not a Botbooru post id")) }
}

/// Booru tags use underscores for spaces.
fn booru_tag(tag: &str) -> String {
    tag.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("_")
}

pub(crate) fn search(request: &dto::DiscoverSearchRequest) -> Result<HttpRequest, ApiError> {
    characters_only(request.kind)?;
    let page = page_of(request.cursor.as_deref());
    let tags = request
        .tags
        .iter()
        .map(|tag| booru_tag(tag))
        .chain(request.exclude_tags.iter().map(|tag| format!("-{}", booru_tag(tag))))
        .filter(|tag| tag.trim_start_matches('-').len() > 0)
        .collect::<Vec<_>>()
        .join(" ");
    Ok(HttpRequest::get(format!("{SITE}/posts/"))
        .param("sort", request.sort.as_deref().filter(|s| !s.is_empty()).unwrap_or("latest"))
        .param_if(!tags.is_empty(), "q", tags)
        .param_if(!request.query.trim().is_empty(), "qtext", request.query.trim())
        .param("limit", PAGE_SIZE)
        .param("offset", (page - 1) * PAGE_SIZE)
        .param_if(!request.nsfw, "sfw_only", "true"))
}

fn tags(node: &Value) -> (Vec<String>, bool) {
    let mut names = Vec::new();
    let mut nsfw = false;
    for tag in node.get("tags").and_then(Value::as_array).into_iter().flatten() {
        let name = text(tag, "name");
        if name == "nsfw" || name == "nsfl" {
            nsfw = true;
        }
        if text(tag, "category") != "Auto" && !name.is_empty() {
            names.push(name.replace('_', " "));
        }
    }
    (names, nsfw)
}

fn card(node: &Value) -> Option<dto::DiscoverCard> {
    let id = node.get("id").and_then(Value::as_u64)?.to_string();
    let (tags, nsfw) = tags(node);
    let tagline = opt_text(node, "tagline").or_else(|| opt_text(node, "description_excerpt")).unwrap_or_default();
    Some(dto::DiscoverCard {
        source: DiscoverSource::Botbooru,
        kind: Kind::Character,
        name: opt_text(node, "character_name").unwrap_or_else(|| id.clone()),
        tagline: clip(&tagline, 220),
        avatar_url: opt_text(node, "filename").map(|file| format!("{SITE}/images/preview/320/{file}")),
        creator: opt_text(node, "uploader_name"),
        tags,
        nsfw,
        tokens: count(node, "token_count"),
        stats: dto::DiscoverStats {
            downloads: count(node, "downloads"),
            views: count(node, "views"),
            likes: count(node, "favorite_count"),
            ..Default::default()
        },
        page_url: Some(format!("{SITE}/post/{id}")),
        id,
    })
}

pub(crate) fn parse_search(body: &[u8], request: &dto::DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    let value = json(body, NAME, "search page")?;
    let posts = value.get("posts").and_then(Value::as_array).ok_or_else(|| malformed(NAME, "search page"))?;
    let page = page_of(request.cursor.as_deref());
    let total = count(&value, "total");
    let has_more = !posts.is_empty() && total.map_or(posts.len() as u32 >= PAGE_SIZE, |total| page * PAGE_SIZE < total);
    Ok(dto::DiscoverSearchPage { items: posts.iter().filter_map(card).collect(), next_cursor: next_page(page, has_more), total })
}

pub(crate) fn detail(kind: Kind, id: &str) -> Result<HttpRequest, ApiError> {
    characters_only(kind)?;
    Ok(HttpRequest::get(format!("{SITE}/post/{}", checked_id(id)?)))
}

pub(crate) fn parse_detail(body: &[u8]) -> Result<dto::DiscoverDetail, ApiError> {
    let node = json(body, NAME, "card")?;
    let mut card = card(&node).ok_or_else(|| malformed(NAME, "card"))?;
    if let Some(file) = opt_text(&node, "filename") {
        card.avatar_url = Some(format!("{SITE}/images/preview/640/{file}"));
    }
    let description = ["creator_notes_display", "creator_notes", "uploader_notes", "tagline"]
        .iter()
        .map(|key| html_to_text(&text(&node, key)))
        .find(|text| !text.is_empty())
        .unwrap_or_default();
    Ok(dto::DiscoverDetail {
        card,
        description,
        greeting: opt_text(&node, "first_mes"),
        alternate_greetings: array_len(&node, "alternate_greetings"),
        has_lorebook: node.get("has_lorebook").and_then(Value::as_bool),
        created_at: opt_text(&node, "created_at"),
        updated_at: opt_text(&node, "content_updated_at"),
        import_blocker: None,
    })
}

pub(crate) async fn download(fetch: &impl Fetch, kind: Kind, id: &str) -> Result<Downloaded, ApiError> {
    characters_only(kind)?;
    let id = checked_id(id)?;
    let file = fetch.fetch(NAME, HttpRequest::file(format!("{SITE}/download/png/{id}"))).await?;
    if !super::card_png::is_png(&file.body) {
        return Err(malformed(NAME, "card file"));
    }
    Ok(Downloaded { bytes: file.body, file_name: file_name(&format!("botbooru_{id}"), "png") })
}
