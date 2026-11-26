//! Parser and flow tests against responses recorded from the live hubs on
//! 2026-11-26 (Chub's are synthetic: the hub was unreachable when recording).

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::sync::Mutex;

use lettuce_contracts::{
    ApiError, ApiErrorCode, DiscoverItemRequest, DiscoverKind as Kind, DiscoverSearchRequest, DiscoverSource as Source,
};
use lettuce_network::JsonResponse;
use serde_json::{Value, json};

use super::card_png::{is_png, read_card, write_card_png};
use super::common::{HttpRequest, error};
use super::{Fetch, Fetched, check_status, detail_with, download_with, search_with};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!("fixtures/", $name)).as_slice()
    };
}

/// Answers by URL prefix and records every request.
struct Fake {
    routes: Vec<(&'static str, Vec<u8>)>,
    seen: Mutex<Vec<HttpRequest>>,
}

impl Fake {
    fn new(routes: Vec<(&'static str, Vec<u8>)>) -> Self {
        Self { routes, seen: Mutex::new(Vec::new()) }
    }
    fn seen(&self) -> Vec<HttpRequest> {
        self.seen.lock().unwrap().clone()
    }
}

impl Fetch for Fake {
    async fn fetch(&self, source: &str, request: HttpRequest) -> Result<Fetched, ApiError> {
        self.seen.lock().unwrap().push(request.clone());
        self.routes
            .iter()
            .find(|(prefix, _)| request.url.starts_with(prefix))
            .map(|(_, body)| Fetched { body: body.clone(), content_type: None })
            .ok_or_else(|| error(ApiErrorCode::NotFound, format!("{source}: no route for {}", request.url)))
    }
}

fn search_request(source: Source, kind: Kind) -> DiscoverSearchRequest {
    DiscoverSearchRequest {
        source,
        kind,
        query: "elf".into(),
        tags: vec!["fantasy".into()],
        exclude_tags: vec!["gore".into()],
        sort: None,
        cursor: None,
        nsfw: true,
        nsfl: false,
    }
}

fn item(source: Source, kind: Kind, id: &str) -> DiscoverItemRequest {
    DiscoverItemRequest { source, kind, id: id.into() }
}

fn param<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request.query.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
}

fn tiny_png() -> Vec<u8> {
    write_card_png(None, &json!({"spec": "chara_card_v2", "data": {"name": "x"}})).unwrap()
}

fn run<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(future)
}

// ---------- card PNG ----------

#[test]
fn card_png_round_trips_and_replaces_old_metadata() {
    let card = json!({"spec": "chara_card_v3", "spec_version": "3.0", "data": {"name": "Привет ✨"}});
    let png = write_card_png(None, &card).unwrap();
    assert!(is_png(&png));
    assert_eq!(read_card(&png).unwrap()["data"]["name"], "Привет ✨");
    let replaced = write_card_png(Some(&png), &json!({"spec": "chara_card_v2", "data": {"name": "B"}})).unwrap();
    assert_eq!(read_card(&replaced).unwrap()["data"]["name"], "B");
    let decoded = image::load_from_memory(&replaced).unwrap();
    assert!(decoded.width() > 0);
}

#[test]
fn card_png_converts_other_image_formats() {
    let mut jpeg = Vec::new();
    image::DynamicImage::new_rgb8(4, 4)
        .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
        .unwrap();
    let png = write_card_png(Some(&jpeg), &json!({"spec": "chara_card_v2", "data": {"name": "J"}})).unwrap();
    assert!(is_png(&png));
    assert_eq!(image::load_from_memory(&png).unwrap().width(), 4);
}

// ---------- Chub (synthetic fixtures) ----------

#[test]
fn chub_search_request_and_parse() {
    let fake = Fake::new(vec![("https://gateway.chub.ai/search", fixture!("chub_search_synthetic.json").to_vec())]);
    let page = run(search_with(&fake, &search_request(Source::Chub, Kind::Character))).unwrap();
    let sent = &fake.seen()[0];
    assert_eq!(param(sent, "search"), Some("elf"));
    assert_eq!(param(sent, "topics"), Some("fantasy"));
    assert_eq!(param(sent, "excludetopics"), Some("gore"));
    assert_eq!(param(sent, "nsfw"), Some("true"));
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.total, Some(2));
    assert_eq!(page.next_cursor, None);
    let first = &page.items[0];
    assert_eq!(first.id, "crystal/eira-the-witch");
    assert!(first.nsfw);
    assert_eq!(first.stats.stars, Some(120));
    assert_eq!(first.creator.as_deref(), Some("crystal"));
    assert_eq!(page.items[1].stats.stars, Some(7));
    assert_eq!(page.items[1].avatar_url.as_deref(), Some("https://avatars.charhub.io/avatars/anon/robot-butler/avatar.webp"));
}

#[test]
fn chub_download_rebuilds_card_with_st_field_mapping() {
    let fake = Fake::new(vec![
        ("https://gateway.chub.ai/api/characters/", fixture!("chub_character_synthetic.json").to_vec()),
        ("https://avatars.charhub.io/", tiny_png()),
    ]);
    let detail = run(detail_with(&fake, &item(Source::Chub, Kind::Character, "crystal/eira-the-witch"))).unwrap();
    assert_eq!(detail.greeting.as_deref(), Some("*The book is upside down.*"));
    assert_eq!(detail.has_lorebook, Some(true));
    assert_eq!(detail.alternate_greetings, 1);
    let file = run(download_with(&fake, &item(Source::Chub, Kind::Character, "crystal/eira-the-witch"))).unwrap();
    let card = read_card(&file.bytes).unwrap();
    assert_eq!(card["data"]["description"], "Eira is a centuries-old witch.");
    assert_eq!(card["data"]["personality"], "sharp-tongued, shy");
    assert_eq!(card["data"]["creator_notes"], "Creator notes here");
    assert_eq!(card["data"]["character_book"]["entries"][0]["keys"][0], "spire");
    assert!(file.file_name.ends_with(".png"));
}

#[test]
fn chub_refuses_paths_that_could_escape_the_api() {
    let fake = Fake::new(Vec::new());
    for bad in ["../etc", "a/b/c", "a/..", "a%2fb/c", ""] {
        let result = run(detail_with(&fake, &item(Source::Chub, Kind::Character, bad)));
        assert_eq!(result.unwrap_err().code, ApiErrorCode::Unsupported, "{bad}");
    }
    assert!(fake.seen().is_empty());
}

#[test]
fn chub_nsfl_is_separate_from_adult() {
    let fake = Fake::new(vec![("https://gateway.chub.ai/search", fixture!("chub_search_synthetic.json").to_vec())]);
    let mut request = search_request(Source::Chub, Kind::Character);
    for (nsfw, nsfl, expect_nsfw, expect_nsfl) in
        [(true, false, "true", "false"), (true, true, "true", "true"), (false, true, "false", "false")]
    {
        request.nsfw = nsfw;
        request.nsfl = nsfl;
        run(search_with(&fake, &request)).unwrap();
        let sent = fake.seen().pop().unwrap();
        assert_eq!(param(&sent, "nsfw"), Some(expect_nsfw));
        assert_eq!(param(&sent, "nsfl"), Some(expect_nsfl));
    }
}

// ---------- Character Tavern ----------

#[test]
fn tavern_search_and_detail_parse_devalue() {
    let fake = Fake::new(vec![
        ("https://character-tavern.com/search/", fixture!("tavern_search.devalue").to_vec()),
        ("https://character-tavern.com/character/", fixture!("tavern_detail.devalue").to_vec()),
    ]);
    let page = run(search_with(&fake, &search_request(Source::CharacterTavern, Kind::Character))).unwrap();
    assert_eq!(param(&fake.seen()[0], "query"), Some("elf"));
    assert!(page.items.len() >= 20);
    assert!(page.next_cursor.is_some());
    let first = &page.items[0];
    assert!(first.avatar_url.as_deref().unwrap().starts_with("https://ct-cards.storage.character-tavern.com/"));
    let detail = run(detail_with(&fake, &item(Source::CharacterTavern, Kind::Character, &first.id))).unwrap();
    assert!(!detail.card.name.is_empty());
    assert!(!detail.description.is_empty());
    assert!(detail.card.creator.is_some());
}

// ---------- Wyvern ----------

#[test]
fn wyvern_search_and_lorebook_merge() {
    let fake = Fake::new(vec![("https://app.wyvern.chat/api/characters/public", fixture!("wyvern_search.json").to_vec())]);
    let page = run(search_with(&fake, &search_request(Source::Wyvern, Kind::Character))).unwrap();
    assert_eq!(param(&fake.seen()[0], "show_nsfw"), Some("true"));
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.total, Some(91));
    assert!(page.items[0].name.starts_with("Aelorne"));

    let book: Value = serde_json::from_slice(fixture!("wyvern_lorebook.json")).unwrap();
    let merged = super::wyvern::character_book(&[book]).unwrap();
    assert_eq!(merged["entries"].as_array().unwrap().len(), 2);
    assert!(merged["entries"][0]["keys"].as_array().is_some_and(|keys| !keys.is_empty()));
}

#[test]
fn wyvern_download_embeds_lorebooks() {
    let search: Value = serde_json::from_slice(fixture!("wyvern_search.json")).unwrap();
    let mut character = search["characters"][0].clone();
    character["lorebooks"] = json!(["book1"]);
    let fake = Fake::new(vec![
        ("https://app.wyvern.chat/api/characters/", serde_json::to_vec(&character).unwrap()),
        ("https://app.wyvern.chat/api/lorebooks/", fixture!("wyvern_lorebook.json").to_vec()),
        ("https://", tiny_png()),
    ]);
    let id = character["id"].as_str().unwrap().to_owned();
    let file = run(download_with(&fake, &item(Source::Wyvern, Kind::Character, &id))).unwrap();
    let card = read_card(&file.bytes).unwrap();
    assert!(card["data"]["name"].as_str().unwrap().starts_with("Aelorne"));
    assert_eq!(card["data"]["character_book"]["entries"].as_array().unwrap().len(), 2);
}

// ---------- Botbooru ----------

#[test]
fn botbooru_search_detail_download() {
    let fake = Fake::new(vec![
        ("https://botbooru.com/posts/", fixture!("botbooru_search.json").to_vec()),
        ("https://botbooru.com/post/", fixture!("botbooru_post.json").to_vec()),
        ("https://botbooru.com/download/png/", tiny_png()),
    ]);
    let mut request = search_request(Source::Botbooru, Kind::Character);
    request.nsfw = false;
    let page = run(search_with(&fake, &request)).unwrap();
    let sent = &fake.seen()[0];
    assert_eq!(param(sent, "q"), Some("fantasy -gore"));
    assert_eq!(param(sent, "qtext"), Some("elf"));
    assert_eq!(param(sent, "sfw_only"), Some("true"));
    assert_eq!(page.items[0].name, "Elara Thornheart");
    assert_eq!(page.total, Some(86));
    assert_eq!(page.next_cursor.as_deref(), Some("2"));
    let detail = run(detail_with(&fake, &item(Source::Botbooru, Kind::Character, "99525"))).unwrap();
    assert_eq!(detail.alternate_greetings, 2);
    assert_eq!(detail.has_lorebook, Some(false));
    assert!(detail.greeting.is_some());
    let file = run(download_with(&fake, &item(Source::Botbooru, Kind::Character, "99525"))).unwrap();
    assert!(is_png(&file.bytes));
    assert!(run(detail_with(&fake, &item(Source::Botbooru, Kind::Character, "1/../2"))).is_err());
}

// ---------- Illarin ----------

#[test]
fn illarin_search_cursor_and_unblurred_covers() {
    let fake = Fake::new(vec![("https://illarin.com/api/v1/works", fixture!("illarin_works.json").to_vec())]);
    let mut request = search_request(Source::Illarin, Kind::Character);
    let page = run(search_with(&fake, &request)).unwrap();
    assert_eq!(param(&fake.seen()[0], "type"), Some("character"));
    assert_eq!(param(&fake.seen()[0], "q"), Some("elf tag:fantasy"));
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.total, Some(153));
    assert!(page.items.iter().all(|card| card.nsfw));
    assert!(!page.items[0].avatar_url.as_deref().unwrap().contains("_blurred"));
    request.cursor = page.next_cursor.clone();
    run(search_with(&fake, &request)).unwrap();
    let second = &fake.seen()[1];
    assert_eq!(param(second, "beforeId"), Some("99e07c0d-5fc4-4180-ac26-5b792e77cb15"));
    assert_eq!(param(second, "beforeCount"), Some("0"));
    request.nsfw = false;
    request.cursor = None;
    assert!(run(search_with(&fake, &request)).unwrap().items.is_empty());
}

#[test]
fn illarin_character_and_preset_downloads() {
    let character: Value = serde_json::from_slice(fixture!("illarin_work.json")).unwrap();
    let preset: Value = serde_json::from_slice(fixture!("illarin_preset_work.json")).unwrap();
    let character_id = character["id"].as_str().unwrap().to_owned();
    let preset_id = preset["id"].as_str().unwrap().to_owned();
    let fake = Fake::new(vec![
        ("https://illarin.com/api/v1/works/", fixture!("illarin_work.json").to_vec()),
        ("https://illarin.com/download/", tiny_png()),
    ]);
    let detail = run(detail_with(&fake, &item(Source::Illarin, Kind::Character, &character_id))).unwrap();
    assert_eq!(detail.card.name, "Eira Halvorsdottir");
    assert!(detail.import_blocker.is_none());
    assert!(detail.greeting.is_some());
    let file = run(download_with(&fake, &item(Source::Illarin, Kind::Character, &character_id))).unwrap();
    assert!(fake.seen().last().unwrap().url.ends_with("/chara_card_v3"));
    assert!(file.file_name.ends_with(".png"));

    let fake = Fake::new(vec![
        ("https://illarin.com/api/v1/works/", fixture!("illarin_preset_work.json").to_vec()),
        ("https://illarin.com/download/", b"{\"prompts\":[]}".to_vec()),
    ]);
    let detail = run(detail_with(&fake, &item(Source::Illarin, Kind::Preset, &preset_id))).unwrap();
    assert!(detail.import_blocker.is_none());
    let file = run(download_with(&fake, &item(Source::Illarin, Kind::Preset, &preset_id))).unwrap();
    assert!(fake.seen().last().unwrap().url.ends_with("/preset_sillytavern"));
    assert!(file.file_name.ends_with(".json"));

    let mut lumiverse_only = preset.clone();
    lumiverse_only["downloads"] = json!([{"format": "preset_lumiverse", "label": "Lumiverse preset"}]);
    let fake = Fake::new(vec![("https://illarin.com/api/v1/works/", serde_json::to_vec(&lumiverse_only).unwrap())]);
    let detail = run(detail_with(&fake, &item(Source::Illarin, Kind::Preset, &preset_id))).unwrap();
    assert_eq!(detail.import_blocker.as_deref(), Some("Only offered as: Lumiverse preset"));
    assert_eq!(run(download_with(&fake, &item(Source::Illarin, Kind::Preset, &preset_id))).unwrap_err().code, ApiErrorCode::Unsupported);
}

// ---------- status mapping ----------

fn response(status: u16, body: &str) -> JsonResponse {
    JsonResponse { status, body: body.as_bytes().to_vec(), request_id: None, retry_after: None, content_type: None }
}

#[test]
fn statuses_map_to_explainable_errors() {
    assert!(check_status("X", response(200, "{}")).is_ok());
    assert_eq!(check_status("X", response(429, "")).err().unwrap().code, ApiErrorCode::Busy);
    assert_eq!(check_status("X", response(404, "")).err().unwrap().code, ApiErrorCode::NotFound);
    let cloudflare = check_status("Some hub", response(403, "<title>Just a moment...</title>")).err().unwrap();
    assert_eq!(cloudflare.code, ApiErrorCode::Unavailable);
    assert!(cloudflare.message.contains("Cloudflare"));
    let region = check_status("Chub", response(403, "This service is not available in your country")).err().unwrap();
    assert!(region.message.contains("region"));
}

#[test]
fn oversized_queries_are_refused() {
    let fake = Fake::new(Vec::new());
    let mut request = search_request(Source::Wyvern, Kind::Character);
    request.query = "x".repeat(500);
    assert_eq!(run(search_with(&fake, &request)).unwrap_err().code, ApiErrorCode::InvalidInput);
    let request = search_request(Source::Wyvern, Kind::Preset);
    assert_eq!(run(search_with(&fake, &request)).unwrap_err().code, ApiErrorCode::Unsupported);
}

/// Hits the real hubs: `cargo test -p lettuce-app fork_discover::tests::live -- --ignored --nocapture`.
#[test]
#[ignore = "talks to the live hubs"]
fn live_search_detail_download() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    for source in [Source::Chub, Source::CharacterTavern, Source::Wyvern, Source::Botbooru, Source::Illarin] {
        let mut request = search_request(source, Kind::Character);
        request.query.clear();
        request.tags.clear();
        request.exclude_tags.clear();
        let page = match runtime.block_on(super::search(&request)) {
            Ok(page) => page,
            Err(failure) => {
                println!("{source:?}: search failed: {failure:?}");
                continue;
            }
        };
        println!("{source:?}: {} items, total {:?}, next {:?}", page.items.len(), page.total, page.next_cursor);
        let Some(first) = page.items.first() else { continue };
        let target = item(source, Kind::Character, &first.id);
        match runtime.block_on(super::detail(&target)) {
            Ok(detail) => println!("  detail {:?}: greeting {} chars, blocker {:?}", detail.card.name, detail.greeting.map_or(0, |g| g.len()), detail.import_blocker),
            Err(failure) => println!("  detail failed: {failure:?}"),
        }
        match runtime.block_on(super::download(&target)) {
            Ok(file) => {
                println!("  download {} ({} bytes), card name {:?}", file.file_name, file.bytes.len(), read_card(&file.bytes).map(|card| card["data"]["name"].clone()));
                save_live(&format!("{source:?}"), &file);
            }
            Err(failure) => println!("  download failed: {failure:?}"),
        }
    }
    for (source, kind) in [(Source::Chub, Kind::Lorebook), (Source::Illarin, Kind::Lorebook), (Source::Illarin, Kind::Preset)] {
        let mut request = search_request(source, kind);
        request.query.clear();
        request.tags.clear();
        let page = runtime.block_on(super::search(&request)).unwrap();
        println!("{source:?} {kind:?}: {} items", page.items.len());
        if let Some(first) = page.items.first() {
            let target = item(source, kind, &first.id);
            let detail = runtime.block_on(super::detail(&target)).unwrap();
            println!("  {:?} blocker {:?}", detail.card.name, detail.import_blocker);
            if detail.import_blocker.is_none() {
                let file = runtime.block_on(super::download(&target)).unwrap();
                println!("  download {} ({} bytes)", file.file_name, file.bytes.len());
                save_live(&format!("{source:?}-{kind:?}"), &file);
            }
        }
    }
}

const LIVE_DIR: &str = "/tmp/lettuce-discover-live";

fn save_live(tag: &str, file: &super::Downloaded) {
    std::fs::create_dir_all(LIVE_DIR).unwrap();
    std::fs::write(format!("{LIVE_DIR}/{tag}-{}", file.file_name), &file.bytes).unwrap();
}

/// Feeds what `live_search_detail_download` saved through the app's own
/// import preview, the path the Discover import button takes.
#[tokio::test]
#[ignore = "needs files from the live test"]
async fn live_files_pass_import_preview() {
    let harness = crate::api::tests::harness(crate::api::tests::Reply::Text("x"));
    let mut names: Vec<_> = std::fs::read_dir(LIVE_DIR).unwrap().map(|entry| entry.unwrap().path()).collect();
    names.sort();
    for path in names {
        let source = lettuce_contracts::FileSource { uri: path.display().to_string() };
        let result = crate::api::import_preview(&harness.context, lettuce_contracts::ImportPreviewRequest { source }).await;
        match result {
            Ok(preview) => println!("{}: {:?} ok", path.display(), preview.kind),
            Err(failure) => println!("{}: FAILED {failure:?}", path.display()),
        }
    }
}

/// Writes safe-for-work search pages and details as JSON for the UI
/// screenshot harness.
#[test]
#[ignore = "talks to the live hubs"]
fn live_dump_for_screenshots() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let out = format!("{LIVE_DIR}/ui");
    std::fs::create_dir_all(&out).unwrap();
    let cases = [
        (Source::Chub, Kind::Character),
        (Source::CharacterTavern, Kind::Character),
        (Source::Wyvern, Kind::Character),
        (Source::Botbooru, Kind::Character),
        (Source::Illarin, Kind::Character),
        (Source::Chub, Kind::Lorebook),
        (Source::Illarin, Kind::Lorebook),
        (Source::Illarin, Kind::Preset),
    ];
    for (source, kind) in cases {
        let mut request = search_request(source, kind);
        request.query.clear();
        request.tags.clear();
        request.exclude_tags.clear();
        request.nsfw = false;
        let Ok(page) = runtime.block_on(super::search(&request)) else { continue };
        let tag = format!("{source:?}-{kind:?}").to_lowercase();
        std::fs::write(format!("{out}/{tag}-search.json"), serde_json::to_vec(&page).unwrap()).unwrap();
        let details: Vec<_> = page
            .items
            .iter()
            .filter(|card| !card.nsfw)
            .take(3)
            .filter_map(|card| runtime.block_on(super::detail(&item(source, kind, &card.id))).ok())
            .collect();
        std::fs::write(format!("{out}/{tag}-details.json"), serde_json::to_vec(&details).unwrap()).unwrap();
        println!("{tag}: {} items, {} details", page.items.len(), details.len());
    }
}
