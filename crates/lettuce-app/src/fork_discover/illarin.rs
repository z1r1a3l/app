//! Illarin (illarin.com), a hub for SillyTavern/Lumiverse/RisuAI works:
//! characters, lorebooks, prompt presets, extensions and themes. Its public
//! `/api/v1/works` JSON lists and describes works; `/download/<id>/<format>`
//! serves each offered format without an account. Pagination is by cursor.
//! Adult works are listed for anonymous visitors with blurred covers; the
//! unblurred media variants are public too.

use std::collections::BTreeMap;

use lettuce_contracts::{self as dto, ApiError, DiscoverKind as Kind, DiscoverSource};
use serde_json::Value;

use super::common::{
    HttpRequest, absolute, clip, count, file_name, json, malformed, opt_text, strings, text, unsupported,
};
use super::{Downloaded, Fetch};

const SITE: &str = "https://illarin.com";
const PAGE_SIZE: u32 = 24;
const NAME: &str = "Illarin";

fn work_type(kind: Kind) -> &'static str {
    match kind {
        Kind::Character => "character",
        Kind::Lorebook => "lorebook",
        Kind::Preset => "preset",
    }
}

/// Download formats this app can import, best first.
fn importable_formats(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Character => &["chara_card_v3", "chara_card_v2"],
        Kind::Lorebook => &["lorebook"],
        // Only SillyTavern prompt-manager presets map onto Lettuce prompts.
        Kind::Preset => &["preset_sillytavern"],
    }
}

fn checked_id(id: &str) -> Result<&str, ApiError> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        Ok(id)
    } else {
        Err(unsupported("not an Illarin work id"))
    }
}

/// Covers come as `/media/<id>/<variant>_blurred/<n>` for anonymous viewers
/// of adult works; the plain variant is served too.
fn media(url: &str) -> String {
    absolute(SITE, &url.replace("_blurred/", "/"))
}

pub(crate) fn search(request: &dto::DiscoverSearchRequest) -> Result<HttpRequest, ApiError> {
    let mut query: Vec<String> = Vec::new();
    if !request.query.trim().is_empty() {
        query.push(request.query.trim().to_owned());
    }
    query.extend(request.tags.iter().map(|tag| tag.trim()).filter(|tag| !tag.is_empty()).map(|tag| format!("tag:{}", tag.to_lowercase())));
    let mut http = HttpRequest::get(format!("{SITE}/api/v1/works"))
        .param("type", work_type(request.kind))
        .param("limit", PAGE_SIZE)
        .param_if(!query.is_empty(), "q", query.join(" "));
    if let Some(sort) = request.sort.as_deref().filter(|sort| !sort.is_empty() && *sort != "new") {
        http = http.param("sort", sort);
    }
    if let Some(cursor) = request.cursor.as_deref() {
        let fields: BTreeMap<String, Value> = serde_json::from_str(cursor).map_err(|_| unsupported("stale Illarin page cursor"))?;
        for (name, value) in fields {
            let value = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
            http = http.param(&name, value);
        }
    }
    Ok(http)
}

fn card(item: &Value, kind: Kind) -> Option<dto::DiscoverCard> {
    let id = text(item, "id");
    checked_id(&id).ok()?;
    let creator = opt_text(item, "creator");
    let apps = strings(item, "apps");
    Some(dto::DiscoverCard {
        source: DiscoverSource::Illarin,
        kind,
        name: opt_text(item, "name").unwrap_or_else(|| id.clone()),
        tagline: opt_text(item, "blurb").map(|blurb| clip(&blurb, 220)).unwrap_or_else(|| apps.join(" · ")),
        avatar_url: item.get("cover").and_then(|cover| opt_text(cover, "url")).map(|url| media(&url)),
        tags: strings(item, "tags"),
        nsfw: item.get("isNsfw").and_then(Value::as_bool).unwrap_or(false),
        tokens: None,
        stats: dto::DiscoverStats { downloads: count(item, "downloadCount"), views: count(item, "viewCount"), ..Default::default() },
        page_url: creator.as_ref().map(|creator| format!("{SITE}/@{creator}")),
        creator,
        id,
    })
}

pub(crate) fn parse_search(body: &[u8], request: &dto::DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    let value = json(body, NAME, "search page")?;
    let items = value.get("items").and_then(Value::as_array).ok_or_else(|| malformed(NAME, "search page"))?;
    let mut cards: Vec<_> = items.iter().filter_map(|item| card(item, request.kind)).collect();
    if !request.nsfw {
        cards.retain(|card| !card.nsfw);
    }
    let next_cursor = value
        .get("nextCursor")
        .filter(|cursor| cursor.is_object())
        .filter(|_| !items.is_empty())
        .map(Value::to_string);
    Ok(dto::DiscoverSearchPage { items: cards, next_cursor, total: count(&value, "total") })
}

pub(crate) fn detail(_kind: Kind, id: &str) -> Result<HttpRequest, ApiError> {
    Ok(HttpRequest::get(format!("{SITE}/api/v1/works/{}", checked_id(id)?)))
}

/// The text of the first block element with `role`.
fn element(work: &Value, role: &str) -> Option<Value> {
    work.get("blocks")?
        .as_array()?
        .iter()
        .flat_map(|block| block.get("elements").and_then(Value::as_array).cloned().unwrap_or_default())
        .find(|element| text(element, "role") == role)
        .and_then(|element| element.get("content").cloned())
}

fn formats(work: &Value) -> Vec<String> {
    work.get("downloads")
        .and_then(Value::as_array)
        .map(|downloads| downloads.iter().map(|download| text(download, "format")).filter(|f| !f.is_empty()).collect())
        .unwrap_or_default()
}

fn pick_format(work: &Value, kind: Kind) -> Option<&'static str> {
    let offered = formats(work);
    importable_formats(kind).iter().copied().find(|format| offered.iter().any(|offered| offered == format))
}

pub(crate) fn parse_detail(body: &[u8], kind: Kind) -> Result<dto::DiscoverDetail, ApiError> {
    let work = json(body, NAME, "work")?;
    let mut card = card(&work, kind).ok_or_else(|| malformed(NAME, "work"))?;
    if let Some(media_items) = work.get("media").and_then(Value::as_array)
        && let Some(cover) = media_items.iter().find(|m| m.get("isCover").and_then(Value::as_bool) == Some(true)).or(media_items.first())
        && let Some(url) = opt_text(cover, "detailUrl")
    {
        card.avatar_url = Some(media(&url));
    }
    let greetings = element(&work, "greetings");
    let greeting_texts: Vec<String> = greetings
        .as_ref()
        .and_then(|content| content.get("texts").and_then(Value::as_array).cloned())
        .map(|texts| texts.iter().map(|t| text(t, "text")).filter(|t| !t.is_empty()).collect())
        .unwrap_or_default();
    let description = opt_text(&work, "blurb")
        .or_else(|| element(&work, "description").and_then(|content| opt_text(&content, "text")).map(|text| clip(&text, 1200)))
        .unwrap_or_default();
    let blocker = pick_format(&work, kind).is_none().then(|| {
        let offered = work
            .get("downloads")
            .and_then(Value::as_array)
            .map(|d| d.iter().filter_map(|d| opt_text(d, "label")).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        if offered.is_empty() { "Illarin offers no download for this work".to_owned() } else { format!("Only offered as: {offered}") }
    });
    Ok(dto::DiscoverDetail {
        card,
        description,
        greeting: greeting_texts.first().cloned(),
        alternate_greetings: u32::try_from(greeting_texts.len().saturating_sub(1)).unwrap_or(0),
        has_lorebook: None,
        created_at: opt_text(&work, "createdAt"),
        updated_at: work.get("latestUpdate").and_then(|update| opt_text(update, "recordedAt")),
        import_blocker: blocker,
    })
}

pub(crate) async fn download(fetch: &impl Fetch, kind: Kind, id: &str) -> Result<Downloaded, ApiError> {
    let id = checked_id(id)?;
    let work = json(&fetch.fetch(NAME, detail(kind, id)?).await?.body, NAME, "work")?;
    let format = pick_format(&work, kind).ok_or_else(|| unsupported("Illarin does not offer this work in a format Lettuce can import"))?;
    let file = fetch.fetch(NAME, HttpRequest::file(format!("{SITE}/download/{id}/{format}"))).await?;
    let extension = if format.starts_with("chara_card") { "png" } else { "json" };
    Ok(Downloaded { bytes: file.body, file_name: file_name(&text(&work, "name"), extension) })
}
