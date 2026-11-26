//! Character Tavern (character-tavern.com). The site is SvelteKit; its
//! `__data.json` route data carries search results and card pages in the
//! devalue format. Card PNGs (V3 with a V2-compatible `chara` chunk) are on a
//! public storage host. No account is needed.

use lettuce_contracts::{self as dto, ApiError, DiscoverKind as Kind, DiscoverSource};
use serde_json::Value;

use super::common::{
    HttpRequest, array_len, clip, comma_list, count, file_name, malformed, next_page, opt_text, page_of, strings, text,
    unsupported,
};
use super::devalue;
use super::{Downloaded, Fetch};

const SITE: &str = "https://character-tavern.com";
const CARDS: &str = "https://ct-cards.storage.character-tavern.com";
const NAME: &str = "Character Tavern";

fn characters_only(kind: Kind) -> Result<(), ApiError> {
    if kind == Kind::Character { Ok(()) } else { Err(unsupported("Character Tavern browsing covers characters only")) }
}

fn checked_path(id: &str) -> Result<&str, ApiError> {
    let valid = id.split('/').count() == 2
        && id.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && part.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c)));
    if valid { Ok(id) } else { Err(unsupported("not a Character Tavern card path")) }
}

fn thumbnail(path: &str) -> String {
    format!("{CARDS}/{path}.png?width=400&quality=80&format=auto")
}

pub(crate) fn search(request: &dto::DiscoverSearchRequest) -> Result<HttpRequest, ApiError> {
    characters_only(request.kind)?;
    let page = page_of(request.cursor.as_deref());
    let tags = comma_list(&request.tags);
    let excluded = comma_list(&request.exclude_tags);
    Ok(HttpRequest::get(format!("{SITE}/search/cards/__data.json"))
        .param_if(!request.query.trim().is_empty(), "query", request.query.trim())
        .param("page", page)
        .param("sort", request.sort.as_deref().filter(|s| !s.is_empty()).unwrap_or("best"))
        .param_if(!tags.is_empty(), "tags", tags)
        .param_if(!excluded.is_empty(), "exclude_tags", excluded)
        // Only the page node: skips the layout and the large tag catalogue.
        .param("x-sveltekit-invalidated", "001"))
}

fn card(hit: &Value, tags: Vec<String>) -> Option<dto::DiscoverCard> {
    let path = text(hit, "path");
    if checked_path(&path).is_err() {
        return None;
    }
    let warnings = strings(hit, "contentWarnings");
    Some(dto::DiscoverCard {
        source: DiscoverSource::CharacterTavern,
        kind: Kind::Character,
        name: opt_text(hit, "name").unwrap_or_else(|| path.clone()),
        tagline: clip(&text(hit, "tagline"), 220),
        avatar_url: Some(thumbnail(&path)),
        creator: opt_text(hit, "author").or_else(|| path.split('/').next().map(str::to_owned)),
        nsfw: hit.get("isNSFW").and_then(Value::as_bool).unwrap_or(false),
        tags: if tags.is_empty() { warnings } else { tags },
        tokens: count(hit, "permanentTokens").or_else(|| count(hit, "tokenTotal")),
        stats: dto::DiscoverStats {
            chats: count(hit, "analytics_chats"),
            messages: count(hit, "analytics_messages"),
            ..Default::default()
        },
        page_url: Some(format!("{SITE}/character/{path}")),
        id: path,
    })
}

pub(crate) fn parse_search(body: &[u8], request: &dto::DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    let data = devalue::parse(body).ok_or_else(|| malformed(NAME, "search page"))?;
    let results = data.page().and_then(|page| page.get("searchResults")).ok_or_else(|| malformed(NAME, "search page"))?;
    let hits = results.get("hits").and_then(Value::as_array).cloned().unwrap_or_default();
    let page = count(results, "page").unwrap_or_else(|| page_of(request.cursor.as_deref()));
    let pages = count(results, "totalPages").unwrap_or(page);
    Ok(dto::DiscoverSearchPage {
        items: hits.iter().filter_map(|hit| card(hit, Vec::new())).collect(),
        next_cursor: next_page(page, !hits.is_empty() && page < pages),
        total: count(results, "totalHits"),
    })
}

pub(crate) fn detail(kind: Kind, id: &str) -> Result<HttpRequest, ApiError> {
    characters_only(kind)?;
    Ok(HttpRequest::get(format!("{SITE}/character/{}/__data.json", checked_path(id)?)).param("x-sveltekit-invalidated", "001"))
}

pub(crate) fn parse_detail(body: &[u8]) -> Result<dto::DiscoverDetail, ApiError> {
    let data = devalue::parse(body).ok_or_else(|| malformed(NAME, "card page"))?;
    let page = data.page().ok_or_else(|| malformed(NAME, "card page"))?;
    let character = page.get("character").ok_or_else(|| malformed(NAME, "card page"))?;
    let mut card = card(character, strings(page, "tags")).ok_or_else(|| malformed(NAME, "card page"))?;
    if card.creator.is_none() {
        card.creator = opt_text(page, "authorUsername");
    }
    card.creator = opt_text(page, "authorUsername").or(card.creator);
    card.tokens = count(character, "tokenTotal").or(card.tokens);
    card.stats.likes = page.get("likesData").and_then(|likes| count(likes, "likeCount"));
    let greetings = page.get("alternativeGreetings").and_then(|value| data.resolve(value));
    let lorebook = page.get("lorebook").and_then(|value| data.resolve(value));
    Ok(dto::DiscoverDetail {
        description: text(character, "description"),
        greeting: opt_text(character, "definition_first_message"),
        alternate_greetings: greetings.map_or(0, |value| match value {
            Value::Array(items) => u32::try_from(items.len()).unwrap_or(0),
            other => array_len(other, "alternativeGreetings"),
        }),
        has_lorebook: Some(
            !character.get("lorebookId").is_none_or(Value::is_null)
                || lorebook.is_some_and(|book| book.get("entries").and_then(Value::as_array).is_some_and(|e| !e.is_empty())),
        ),
        created_at: opt_text(character, "createdAt"),
        updated_at: opt_text(character, "lastUpdatedAt"),
        import_blocker: None,
        card,
    })
}

pub(crate) async fn download(fetch: &impl Fetch, kind: Kind, id: &str) -> Result<Downloaded, ApiError> {
    characters_only(kind)?;
    let path = checked_path(id)?;
    let file = fetch.fetch(NAME, HttpRequest::file(format!("{CARDS}/{path}.png")).param("action", "download")).await?;
    if !super::card_png::is_png(&file.body) {
        return Err(malformed(NAME, "card file"));
    }
    Ok(Downloaded { bytes: file.body, file_name: file_name(path.rsplit('/').next().unwrap_or("card"), "png") })
}
