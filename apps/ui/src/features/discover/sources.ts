// Fork: the hubs the Discover tab can browse, and what each one supports.
import type { DiscoverKind, DiscoverSource } from "@/api/generated/bindings";

export type Capability = "search" | "preview" | "import";
export type SortLabel =
  | "trending" | "downloads" | "stars" | "favorites" | "rating" | "newest" | "updated" | "views"
  | "likes" | "messages" | "best" | "popular" | "hiddenGems" | "relevance" | "tokens" | "random";
export type SourceNote = "pygmalion" | "janny" | "botbooru" | "illarin";
export type Section = "characters" | "resources";

export interface SourceInfo {
  id: DiscoverSource;
  name: string;
  domain: string;
  url: string;
  kinds: readonly DiscoverKind[];
  /** Capabilities per kind; a missing one is shown crossed out in the sources list. */
  capabilities: Partial<Record<DiscoverKind, readonly Capability[]>>;
  /** Sort values the hub understands, each with a label key under `discover.sort`. */
  sorts: Partial<Record<DiscoverKind, readonly { value: string; label: SortLabel }[]>>;
  /** Exclusion tags are ignored by this hub. */
  noExclude?: boolean;
  /** A note key under `discover.sourceNote` shown in the sources list. */
  note?: SourceNote;
}

const all: readonly Capability[] = ["search", "preview", "import"];

export const SOURCES: readonly SourceInfo[] = [
  {
    id: "chub",
    name: "Chub",
    domain: "chub.ai",
    url: "https://chub.ai",
    kinds: ["character", "lorebook"],
    capabilities: { character: all, lorebook: all },
    sorts: {
      character: [
        { value: "default", label: "trending" },
        { value: "download_count", label: "downloads" },
        { value: "star_count", label: "stars" },
        { value: "n_favorites", label: "favorites" },
        { value: "rating", label: "rating" },
        { value: "created_at", label: "newest" },
        { value: "last_activity_at", label: "updated" },
      ],
      lorebook: [
        { value: "default", label: "trending" },
        { value: "star_count", label: "stars" },
        { value: "created_at", label: "newest" },
        { value: "last_activity_at", label: "updated" },
      ],
    },
  },
  {
    id: "pygmalion",
    name: "Pygmalion",
    domain: "pygmalion.chat",
    url: "https://pygmalion.chat",
    kinds: ["character"],
    capabilities: { character: all },
    sorts: {
      character: [
        { value: "downloads", label: "downloads" },
        { value: "stars", label: "stars" },
        { value: "views", label: "views" },
        { value: "approved_at", label: "newest" },
      ],
    },
    note: "pygmalion",
  },
  {
    id: "character_tavern",
    name: "Character Tavern",
    domain: "character-tavern.com",
    url: "https://character-tavern.com",
    kinds: ["character"],
    capabilities: { character: all },
    sorts: {
      character: [
        { value: "best", label: "best" },
        { value: "popular", label: "popular" },
        { value: "most_liked", label: "likes" },
        { value: "newest", label: "newest" },
        { value: "recently_updated", label: "updated" },
        { value: "hidden_gems", label: "hiddenGems" },
      ],
    },
  },
  {
    id: "janny",
    name: "JannyAI",
    domain: "jannyai.com",
    url: "https://jannyai.com",
    kinds: ["character"],
    capabilities: { character: ["search", "import"] },
    sorts: {
      character: [
        { value: "", label: "relevance" },
        { value: "newest", label: "newest" },
        { value: "tokens", label: "tokens" },
      ],
    },
    note: "janny",
  },
  {
    id: "wyvern",
    name: "Wyvern",
    domain: "wyvern.chat",
    url: "https://wyvern.chat",
    kinds: ["character"],
    capabilities: { character: all },
    sorts: {
      character: [
        { value: "total_likes", label: "likes" },
        { value: "total_messages", label: "messages" },
        { value: "total_views", label: "views" },
        { value: "dateCreated", label: "newest" },
      ],
    },
  },
  {
    id: "botbooru",
    name: "Botbooru",
    domain: "botbooru.com",
    url: "https://botbooru.com",
    kinds: ["character"],
    capabilities: { character: all },
    sorts: {
      character: [
        { value: "latest", label: "newest" },
        { value: "downloads", label: "downloads" },
        { value: "favorites", label: "favorites" },
        { value: "views", label: "views" },
        { value: "score", label: "rating" },
        { value: "random", label: "random" },
      ],
    },
    note: "botbooru",
  },
  {
    id: "illarin",
    name: "Illarin",
    domain: "illarin.com",
    url: "https://illarin.com",
    kinds: ["character", "lorebook", "preset"],
    capabilities: { character: all, lorebook: all, preset: all },
    sorts: {
      character: [
        { value: "new", label: "newest" },
        { value: "downloads", label: "downloads" },
        { value: "views", label: "views" },
      ],
    },
    noExclude: true,
    note: "illarin",
  },
];

export function sourceInfo(id: DiscoverSource): SourceInfo {
  return SOURCES.find((source) => source.id === id) ?? SOURCES[0]!;
}

export function sourcesFor(kind: DiscoverKind): SourceInfo[] {
  return SOURCES.filter((source) => source.kinds.includes(kind));
}

export function sortsFor(source: SourceInfo, kind: DiscoverKind) {
  return source.sorts[kind] ?? source.sorts.character ?? [];
}

export const RESOURCE_KINDS: readonly DiscoverKind[] = ["lorebook", "preset"];
