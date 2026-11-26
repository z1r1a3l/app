//! JannyAI (jannyai.com), a mirror of JanitorAI characters. Search goes to
//! its public Meilisearch instance with the read-only key its web client
//! ships. Card pages and the download endpoint sit behind Cloudflare's bot
//! check, which a plain HTTP client usually cannot pass, so import is
//! best-effort: it works only when Cloudflare lets the request through.

use lettuce_contracts::{self as dto, ApiError, ApiErrorCode, DiscoverKind as Kind, DiscoverSource};
use serde_json::{Value, json};

use super::common::{
    HttpRequest, clip, count, error, file_name, html_to_text, json, malformed, next_page, opt_text, page_of, text,
    unsupported,
};
use super::{Downloaded, Fetch};

const SEARCH: &str = "https://search.jannyai.com/multi-search";
/// The public, search-only key from jannyai.com's client bundle.
const SEARCH_KEY: &str = "88a6463b66e04fb07ba87ee3db06af337f492ce511d93df6e2d2968cb2ff2b30";
const IMAGES: &str = "https://image.jannyai.com/bot-avatars";
const SITE: &str = "https://jannyai.com";
const PAGE_SIZE: u32 = 24;
const NAME: &str = "JannyAI";

/// JannyAI's fixed tag ids.
pub(crate) const TAGS: &[(u32, &str)] = &[
    (1, "Male"), (2, "Female"), (3, "Non-binary"), (4, "Celebrity"), (5, "OC"), (6, "Fictional"), (7, "Real"),
    (8, "Game"), (9, "Anime"), (10, "Historical"), (11, "Royalty"), (12, "Detective"), (13, "Hero"), (14, "Villain"),
    (15, "Magical"), (16, "Non-human"), (17, "Monster"), (18, "Monster Girl"), (19, "Alien"), (20, "Robot"),
    (21, "Politics"), (22, "Vampire"), (23, "Giant"), (24, "OpenAI"), (25, "Elf"), (26, "Multiple"), (27, "VTuber"),
    (28, "Dominant"), (29, "Submissive"), (30, "Scenario"), (31, "Pokemon"), (32, "Assistant"), (34, "Non-English"),
    (36, "Philosophy"), (38, "RPG"), (39, "Religion"), (41, "Books"), (42, "AnyPOV"), (43, "Angst"), (44, "Demi-Human"),
    (45, "Enemies to Lovers"), (46, "Smut"), (47, "MLM"), (48, "WLW"), (49, "Action"), (50, "Romance"), (51, "Horror"),
    (52, "Slice of Life"), (53, "Fantasy"), (54, "Drama"), (55, "Comedy"), (56, "Mystery"), (57, "Sci-Fi"),
    (59, "Yandere"), (60, "Furry"), (61, "Movies/TV"),
];

fn tag_id(name: &str) -> Option<u32> {
    TAGS.iter().find(|(_, tag)| tag.eq_ignore_ascii_case(name.trim())).map(|(id, _)| *id)
}

fn tag_name(id: u64) -> Option<&'static str> {
    TAGS.iter().find(|(tag, _)| u64::from(*tag) == id).map(|(_, name)| *name)
}

fn characters_only(kind: Kind) -> Result<(), ApiError> {
    if kind == Kind::Character { Ok(()) } else { Err(unsupported("JannyAI hosts characters only")) }
}

fn checked_id(id: &str) -> Result<&str, ApiError> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        Ok(id)
    } else {
        Err(unsupported("not a JannyAI character id"))
    }
}

pub(crate) fn search(request: &dto::DiscoverSearchRequest) -> Result<HttpRequest, ApiError> {
    characters_only(request.kind)?;
    let page = page_of(request.cursor.as_deref());
    let mut filters = vec![Value::String("totalToken >= 0".to_owned())];
    if !request.nsfw {
        filters.push(json!("isNsfw = false"));
    }
    filters.extend(request.tags.iter().filter_map(|tag| tag_id(tag)).map(|id| json!(format!("tagIds = {id}"))));
    filters.extend(request.exclude_tags.iter().filter_map(|tag| tag_id(tag)).map(|id| json!(format!("NOT tagIds = {id}"))));
    let sort = match request.sort.as_deref() {
        None | Some("" | "newest") => Some("createdAtStamp:desc"),
        Some("tokens") => Some("totalToken:desc"),
        Some(_) => None,
    };
    let mut query = json!({
        "indexUid": "janny-characters",
        "q": request.query.trim(),
        "filter": filters,
        "hitsPerPage": PAGE_SIZE,
        "page": page,
    });
    if let Some(sort) = sort {
        query["sort"] = json!([sort]);
    }
    let mut http = HttpRequest::post(SEARCH, &json!({ "queries": [query] }));
    http.bearer = Some(SEARCH_KEY);
    Ok(http)
}

fn card(hit: &Value) -> Option<dto::DiscoverCard> {
    let id = text(hit, "id");
    if checked_id(&id).is_err() {
        return None;
    }
    let tags = hit
        .get("tagIds")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_u64).filter_map(tag_name).map(str::to_owned).collect())
        .unwrap_or_default();
    let description = html_to_text(&text(hit, "description"));
    Some(dto::DiscoverCard {
        source: DiscoverSource::Janny,
        kind: Kind::Character,
        name: opt_text(hit, "name").unwrap_or_else(|| id.clone()),
        tagline: clip(&description, 600),
        avatar_url: opt_text(hit, "avatar").map(|file| if file.starts_with("http") { file } else { format!("{IMAGES}/{file}") }),
        creator: None,
        tags,
        nsfw: hit.get("isNsfw").and_then(Value::as_bool).unwrap_or(false),
        tokens: count(hit, "totalToken"),
        stats: dto::DiscoverStats::default(),
        page_url: Some(format!("{SITE}/characters/{id}")),
        id,
    })
}

pub(crate) fn parse_search(body: &[u8], request: &dto::DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    let value = json(body, NAME, "search page")?;
    let result = value.get("results").and_then(Value::as_array).and_then(|r| r.first()).ok_or_else(|| malformed(NAME, "search page"))?;
    let hits = result.get("hits").and_then(Value::as_array).cloned().unwrap_or_default();
    let page = count(result, "page").unwrap_or_else(|| page_of(request.cursor.as_deref()));
    let pages = count(result, "totalPages").unwrap_or(page);
    Ok(dto::DiscoverSearchPage {
        items: hits.iter().filter_map(card).collect(),
        next_cursor: next_page(page, !hits.is_empty() && page < pages),
        total: count(result, "totalHits"),
    })
}

pub(crate) async fn download(fetch: &impl Fetch, kind: Kind, id: &str) -> Result<Downloaded, ApiError> {
    characters_only(kind)?;
    let id = checked_id(id)?;
    let answer = fetch
        .fetch(NAME, HttpRequest::post("https://api.jannyai.com/api/v1/download", &json!({ "characterId": id })))
        .await?;
    let answer = json(&answer.body, NAME, "download answer")?;
    let url = (text(&answer, "status") == "ok").then(|| opt_text(&answer, "downloadUrl")).flatten().ok_or_else(|| {
        error(ApiErrorCode::Unavailable, format!("{NAME} did not offer a download for this card"))
    })?;
    let file = fetch.fetch(NAME, HttpRequest::file(url)).await?;
    Ok(Downloaded { bytes: file.body, file_name: file_name(id, "png") })
}
