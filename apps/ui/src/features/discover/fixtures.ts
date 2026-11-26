// Fork: sample hub results for Discover tests and screenshots.
import type { DiscoverCard, DiscoverDetail, DiscoverKind, DiscoverSource } from "@/api/generated/bindings";

const stats = { stars: null, downloads: null, chats: null, messages: null, views: null, likes: null, rating: null };

export function fixtureCard(id: string, name: string, patch: Partial<DiscoverCard> = {}): DiscoverCard {
  const { stats: statsPatch, ...rest } = patch;
  return {
    source: "chub" as DiscoverSource,
    kind: "character" as DiscoverKind,
    id,
    name,
    tagline: "",
    avatar_url: null,
    creator: null,
    tags: [],
    nsfw: false,
    tokens: null,
    page_url: null,
    ...rest,
    stats: { ...stats, ...statsPatch },
  };
}

export function fixtureDetail(card: DiscoverCard, patch: Partial<DiscoverDetail> = {}): DiscoverDetail {
  return {
    card,
    description: card.tagline,
    greeting: null,
    alternate_greetings: 0,
    has_lorebook: null,
    created_at: null,
    updated_at: null,
    import_blocker: null,
    ...patch,
  };
}
