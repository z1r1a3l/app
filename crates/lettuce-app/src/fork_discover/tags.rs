//! Tag suggestions. Chub publishes its tag list (fetched once, kept for a
//! day); Illarin answers prefix searches; the other hubs expose no list, so
//! the UI suggests tags from the results it has shown instead.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use lettuce_contracts::{ApiError, DiscoverSource, DiscoverTag, DiscoverTagsPage, DiscoverTagsRequest};
use serde_json::{Value, json};

use super::Fetch;
use super::common::{HttpRequest, json, opt_text};

const MAX_SUGGESTIONS: usize = 40;
const CHUB_TAG_LIMIT: u32 = 5000;
const KEEP_FOR: Duration = Duration::from_secs(24 * 60 * 60);

static CHUB_TAGS: Mutex<Option<(Instant, Vec<DiscoverTag>)>> = Mutex::new(None);

fn count_of(value: &Value, key: &str) -> Option<u32> {
    value.get(key).and_then(Value::as_u64).and_then(|count| u32::try_from(count).ok())
}

pub(crate) fn parse_chub(body: &[u8]) -> Result<Vec<DiscoverTag>, ApiError> {
    let value = json(body, "Chub", "tags")?;
    let mut seen = std::collections::HashSet::new();
    Ok(value
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tag| {
            let name = opt_text(tag, "name")?.trim().to_owned();
            (!name.is_empty() && seen.insert(name.to_lowercase()))
                .then(|| DiscoverTag { name, count: count_of(tag, "non_private_projects_count") })
        })
        .collect())
}

pub(crate) fn parse_illarin(body: &[u8]) -> Result<Vec<DiscoverTag>, ApiError> {
    let value = json(body, "Illarin", "tags")?;
    Ok(value
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tag| {
            let name = opt_text(tag, "value")?.trim().to_owned();
            (!name.is_empty()).then(|| DiscoverTag { name, count: count_of(tag, "count") })
        })
        .collect())
}

/// Prefix matches first (by popularity, as listed), then other matches.
pub(crate) fn matching(tags: &[DiscoverTag], prefix: &str) -> Vec<DiscoverTag> {
    let needle = prefix.trim().to_lowercase();
    let (mut starts, mut contains) = (Vec::new(), Vec::new());
    for tag in tags {
        let name = tag.name.to_lowercase();
        if needle.is_empty() || name.starts_with(&needle) {
            starts.push(tag.clone());
        } else if name.contains(&needle) {
            contains.push(tag.clone());
        }
    }
    starts.extend(contains);
    starts.truncate(MAX_SUGGESTIONS);
    starts
}

async fn chub_tags(fetch: &impl Fetch) -> Result<Vec<DiscoverTag>, ApiError> {
    if let Some((at, tags)) = CHUB_TAGS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref()
        && at.elapsed() < KEEP_FOR
    {
        return Ok(tags.clone());
    }
    let mut request = HttpRequest::get("https://gateway.chub.ai/tags");
    request.body = Some(json!({ "limit": CHUB_TAG_LIMIT }).to_string());
    let tags = parse_chub(&fetch.fetch("Chub", request).await?.body)?;
    *CHUB_TAGS.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((Instant::now(), tags.clone()));
    Ok(tags)
}

pub(crate) async fn tags_with(fetch: &impl Fetch, request: &DiscoverTagsRequest) -> Result<DiscoverTagsPage, ApiError> {
    let prefix: String = request.prefix.trim().chars().take(60).collect();
    match request.source {
        DiscoverSource::Chub => Ok(DiscoverTagsPage { tags: matching(&chub_tags(fetch).await?, &prefix), unsupported: false }),
        DiscoverSource::Illarin => {
            if prefix.is_empty() {
                return Ok(DiscoverTagsPage { tags: Vec::new(), unsupported: false });
            }
            let request = HttpRequest::get("https://illarin.com/api/v1/tags").param("q", prefix.to_lowercase());
            let tags = parse_illarin(&fetch.fetch("Illarin", request).await?.body)?;
            Ok(DiscoverTagsPage { tags: matching(&tags, &prefix), unsupported: false })
        }
        DiscoverSource::CharacterTavern | DiscoverSource::Wyvern | DiscoverSource::Botbooru => {
            Ok(DiscoverTagsPage { tags: Vec::new(), unsupported: true })
        }
    }
}
