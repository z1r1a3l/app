//! Fork: DTOs for the Discover tab, a browser for public character-card
//! hubs (Chub, Character Tavern, Wyvern, Botbooru, Illarin). Kept in its own file so
//! upstream rebases only touch the one `mod` line in `lib.rs`.

use serde::{Deserialize, Serialize};

use crate::FileSource;

/// A card hub the Discover tab can browse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum DiscoverSource {
    Chub,
    CharacterTavern,
    Wyvern,
    Botbooru,
    Illarin,
}

/// What kind of resource a listing is. Characters are the main tab; the
/// Resources tab browses lorebooks and prompt presets on hubs that host them.
/// Regex scripts, themes and extensions can be added here later without
/// changing the request shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum DiscoverKind {
    Character,
    Lorebook,
    Preset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct DiscoverSearchRequest {
    pub source: DiscoverSource,
    pub kind: DiscoverKind,
    #[serde(default)]
    pub query: String,
    /// Tags every result must carry (names, as the hub spells them).
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub exclude_tags: Vec<String>,
    /// A sort key from the source's own list; `None` is the hub default.
    #[serde(default)]
    pub sort: Option<String>,
    /// The `next_cursor` of the previous page; `None` for the first page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Include adult cards.
    pub nsfw: bool,
    /// Also include NSFL (extreme) cards where the hub tags them (Chub);
    /// only honoured together with `nsfw`.
    #[serde(default)]
    pub nsfl: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiscoverStats {
    pub stars: Option<u32>,
    pub downloads: Option<u32>,
    pub chats: Option<u32>,
    pub messages: Option<u32>,
    pub views: Option<u32>,
    pub likes: Option<u32>,
    pub rating: Option<f32>,
}

/// One search hit, enough for a grid tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiscoverCard {
    pub source: DiscoverSource,
    pub kind: DiscoverKind,
    /// The id the source's detail and download calls take.
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub avatar_url: Option<String>,
    pub creator: Option<String>,
    pub tags: Vec<String>,
    pub nsfw: bool,
    pub tokens: Option<u32>,
    pub stats: DiscoverStats,
    /// The card's public page, for "open on site".
    pub page_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiscoverSearchPage {
    pub items: Vec<DiscoverCard>,
    /// Opaque; pass back as `cursor` for the next page. `None` at the end.
    pub next_cursor: Option<String>,
    pub total: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct DiscoverItemRequest {
    pub source: DiscoverSource,
    pub kind: DiscoverKind,
    pub id: String,
}

/// The detail sheet: the tile plus the long texts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiscoverDetail {
    pub card: DiscoverCard,
    /// The creator's public description (creator notes), as plain text.
    pub description: String,
    /// The first greeting, when the source exposes it before download.
    pub greeting: Option<String>,
    /// Why the item cannot be imported here (e.g. only a Lumiverse preset is
    /// offered), or `None` when Import should work.
    pub import_blocker: Option<String>,
    pub alternate_greetings: u32,
    pub has_lorebook: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// A downloaded card, written to the app's cache and ready for the normal
/// import preview/commit flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiscoverDownload {
    pub source: FileSource,
    pub file_name: String,
}
