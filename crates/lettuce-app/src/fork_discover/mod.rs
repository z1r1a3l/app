//! Fork: the Discover tab's hub browser. Searches public character-card hubs
//! (Chub, Character Tavern, Wyvern, Botbooru, Illarin),
//! reads one card's details, and downloads it as an importable file (card PNG,
//! lorebook JSON or prompt preset JSON) for the regular import flow.
//!
//! Every request goes through `lettuce-network`'s JSON client; nothing from
//! the hubs' web pages is embedded, so no site ads reach the app.

mod botbooru;
mod card_png;
mod chub;
mod common;
mod devalue;
mod illarin;
mod tavern;
mod wyvern;

#[cfg(test)]
mod tests;

use std::future::Future;
use std::sync::OnceLock;

use lettuce_contracts::{
    self as dto, ApiError, ApiErrorCode, DiscoverItemRequest, DiscoverKind as Kind, DiscoverSearchRequest,
    DiscoverSource,
};
use lettuce_network::{
    JsonAuth, JsonClient, JsonClientError, JsonQueryParameter, JsonResponse, JsonStaticHeader, RequestPolicy,
};

use common::{HttpRequest, error};

/// Card files with large embedded art and lorebooks run past the default cap.
const MAX_RESPONSE_BYTES: usize = 48 * 1024 * 1024;
const USER_AGENT: &str = "LettuceAI/1.0 (discover)";

/// A successful (2xx) response body.
pub(crate) struct Fetched {
    pub body: Vec<u8>,
    #[allow(dead_code, reason = "kept for adapters that sniff formats")]
    pub content_type: Option<String>,
}

/// An importable file an adapter produced.
#[derive(Debug)]
pub struct Downloaded {
    pub bytes: Vec<u8>,
    pub file_name: String,
}

/// Makes one request; any non-2xx status is an `ApiError`.
pub(crate) trait Fetch: Sync {
    fn fetch(&self, source: &str, request: HttpRequest) -> impl Future<Output = Result<Fetched, ApiError>> + Send;
}

fn source_name(source: DiscoverSource) -> &'static str {
    match source {
        DiscoverSource::Chub => "Chub",
        DiscoverSource::CharacterTavern => "Character Tavern",
        DiscoverSource::Wyvern => "Wyvern",
        DiscoverSource::Botbooru => "Botbooru",
        DiscoverSource::Illarin => "Illarin",
    }
}

/// One page of search results.
pub async fn search(request: &DiscoverSearchRequest) -> Result<dto::DiscoverSearchPage, ApiError> {
    search_with(&Net, request).await
}

/// Description, greeting and stats for one result.
pub async fn detail(request: &DiscoverItemRequest) -> Result<dto::DiscoverDetail, ApiError> {
    detail_with(&Net, request).await
}

/// The importable file for one result.
pub async fn download(request: &DiscoverItemRequest) -> Result<Downloaded, ApiError> {
    download_with(&Net, request).await
}

pub(crate) async fn search_with(
    fetch: &impl Fetch,
    request: &DiscoverSearchRequest,
) -> Result<dto::DiscoverSearchPage, ApiError> {
    if request.query.len() > 200 || request.tags.len() + request.exclude_tags.len() > 12 {
        return Err(error(ApiErrorCode::InvalidInput, "search query or tag list is too long"));
    }
    let name = source_name(request.source);
    let http = match request.source {
        DiscoverSource::Chub => chub::search(request)?,
        DiscoverSource::CharacterTavern => tavern::search(request)?,
        DiscoverSource::Wyvern => wyvern::search(request)?,
        DiscoverSource::Botbooru => botbooru::search(request)?,
        DiscoverSource::Illarin => illarin::search(request)?,
    };
    let body = fetch.fetch(name, http).await?.body;
    match request.source {
        DiscoverSource::Chub => chub::parse_search(&body, request),
        DiscoverSource::CharacterTavern => tavern::parse_search(&body, request),
        DiscoverSource::Wyvern => wyvern::parse_search(&body, request),
        DiscoverSource::Botbooru => botbooru::parse_search(&body, request),
        DiscoverSource::Illarin => illarin::parse_search(&body, request),
    }
}

pub(crate) async fn detail_with(
    fetch: &impl Fetch,
    request: &DiscoverItemRequest,
) -> Result<dto::DiscoverDetail, ApiError> {
    let (kind, id) = (request.kind, request.id.as_str());
    let name = source_name(request.source);
    let http = match request.source {
        DiscoverSource::Chub => chub::detail(kind, id)?,
        DiscoverSource::CharacterTavern => tavern::detail(kind, id)?,
        DiscoverSource::Wyvern => wyvern::detail(kind, id)?,
        DiscoverSource::Botbooru => botbooru::detail(kind, id)?,
        DiscoverSource::Illarin => illarin::detail(kind, id)?,
    };
    let body = fetch.fetch(name, http).await?.body;
    match request.source {
        DiscoverSource::Chub => chub::parse_detail(&body, kind),
        DiscoverSource::CharacterTavern => tavern::parse_detail(&body),
        DiscoverSource::Wyvern => wyvern::parse_detail(&body),
        DiscoverSource::Botbooru => botbooru::parse_detail(&body),
        DiscoverSource::Illarin => illarin::parse_detail(&body, kind),
    }
}

pub(crate) async fn download_with(fetch: &impl Fetch, request: &DiscoverItemRequest) -> Result<Downloaded, ApiError> {
    let (kind, id) = (request.kind, request.id.as_str());
    match request.source {
        DiscoverSource::Chub => chub::download(fetch, kind, id).await,
        DiscoverSource::CharacterTavern => tavern::download(fetch, kind, id).await,
        DiscoverSource::Wyvern => wyvern::download(fetch, kind, id).await,
        DiscoverSource::Botbooru => botbooru::download(fetch, kind, id).await,
        DiscoverSource::Illarin => illarin::download(fetch, kind, id).await,
    }
}

/// Whether a kind is browsable at a source at all (mirrors the UI table).
#[must_use]
pub fn supports(source: DiscoverSource, kind: Kind) -> bool {
    match kind {
        Kind::Character => true,
        Kind::Lorebook => matches!(source, DiscoverSource::Chub | DiscoverSource::Illarin),
        Kind::Preset => source == DiscoverSource::Illarin,
    }
}

/// The live network.
struct Net;

fn client() -> Result<&'static JsonClient, ApiError> {
    static CLIENT: OnceLock<JsonClient> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let client = JsonClient::new()
        .map_err(|_| error(ApiErrorCode::Internal, "the HTTP client could not be set up"))?
        .with_max_response_bytes(MAX_RESPONSE_BYTES);
    Ok(CLIENT.get_or_init(|| client))
}

impl Fetch for Net {
    async fn fetch(&self, source: &str, request: HttpRequest) -> Result<Fetched, ApiError> {
        let url = url::Url::parse(&request.url).map_err(|_| error(ApiErrorCode::Internal, "bad hub URL"))?;
        let endpoint = url.origin().ascii_serialization();
        let mut pairs: Vec<(String, String)> = url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        pairs.extend(request.query.iter().cloned());
        let query: Vec<JsonQueryParameter<'_>> =
            pairs.iter().map(|(name, value)| JsonQueryParameter { name, value }).collect();
        let mut headers: Vec<JsonStaticHeader> =
            request.headers.iter().map(|&(name, value)| JsonStaticHeader { name, value }).collect();
        headers.push(JsonStaticHeader { name: "user-agent", value: USER_AGENT });
        let auth = JsonAuth::None;
        let client = client()?;
        let path = url.path();
        let response = match request.body {
            Some(body) => {
                client
                    .post_json_with_query(&endpoint, path, &query, body.into_bytes(), &headers, auth, Vec::new(), RequestPolicy::BROWSE)
                    .await
            }
            None => {
                client.get_json_with_query(&endpoint, path, &query, &headers, auth, Vec::new(), RequestPolicy::BROWSE).await
            }
        };
        let response = response.map_err(|failure| transport_error(source, &failure))?;
        check_status(source, response)
    }
}

fn transport_error(source: &str, failure: &JsonClientError) -> ApiError {
    tracing::warn!(source, %failure, "discover request failed");
    match failure {
        JsonClientError::ResponseTooLarge => error(ApiErrorCode::Unsupported, format!("{source} sent a file that is too large")),
        JsonClientError::InvalidUrl { .. } | JsonClientError::InvalidRequest { .. } => {
            error(ApiErrorCode::InvalidInput, format!("could not build a {source} request"))
        }
        _ => error(ApiErrorCode::Unavailable, format!("could not reach {source}; check the connection")),
    }
}

/// Maps hub statuses onto errors the UI can explain.
pub(crate) fn check_status(source: &str, response: JsonResponse) -> Result<Fetched, ApiError> {
    let status = response.status;
    if (200..300).contains(&status) {
        return Ok(Fetched { body: response.body, content_type: response.content_type });
    }
    let body = String::from_utf8_lossy(&response.body[..response.body.len().min(16 * 1024)]).to_lowercase();
    tracing::warn!(source, status, "discover request refused");
    Err(match status {
        429 => error(ApiErrorCode::Busy, format!("{source} is rate-limiting requests; wait a minute and retry")),
        404 | 410 => error(ApiErrorCode::NotFound, format!("{source} no longer has this")),
        _ if body.contains("not available in your country") || body.contains("unavailable in your region") => {
            error(ApiErrorCode::Unavailable, format!("{source} is not available from your region"))
        }
        _ if body.contains("just a moment") || body.contains("challenge-platform") || body.contains("attention required") => {
            error(ApiErrorCode::Unavailable, format!("{source} answered with a Cloudflare bot check that only a web browser can pass"))
        }
        401 | 403 => error(ApiErrorCode::Unavailable, format!("{source} refused the request ({status})")),
        500..=599 => error(ApiErrorCode::Unavailable, format!("{source} is having trouble ({status}); try again later")),
        _ => error(ApiErrorCode::Unavailable, format!("{source} answered {status}")),
    })
}
