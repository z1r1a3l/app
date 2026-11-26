// Fork: Discover queries and the download → import hand-off.
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import type { ApiClient } from "@/api/client";
import type {
  DiscoverCard,
  DiscoverItemRequest,
  DiscoverKind,
  DiscoverSearchRequest,
  DiscoverSource,
  FileSource,
  ImportPreview_Serialize,
} from "@/api/generated/bindings";

export interface Filters {
  source: DiscoverSource;
  kind: DiscoverKind;
  query: string;
  tags: string[];
  excludeTags: string[];
  sort: string | null;
  nsfw: boolean;
  /** NSFL cards (Chub only); only sent together with `nsfw`. */
  nsfl: boolean;
}

const discoverKeys = {
  all: ["fork-discover"] as const,
  search: (filters: Filters) => ["fork-discover", "search", filters] as const,
  detail: (item: DiscoverItemRequest) => ["fork-discover", "detail", item.source, item.kind, item.id] as const,
};

export function useDiscoverSearch(api: ApiClient, filters: Filters) {
  return useInfiniteQuery({
    queryKey: discoverKeys.search(filters),
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => {
      const request: DiscoverSearchRequest = {
        source: filters.source,
        kind: filters.kind,
        query: filters.query,
        tags: filters.tags,
        exclude_tags: filters.excludeTags,
        sort: filters.sort,
        cursor: pageParam,
        nsfw: filters.nsfw,
        nsfl: filters.nsfw && filters.nsfl,
      };
      return api.call("discoverSearch", request);
    },
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    staleTime: 5 * 60_000,
    retry: 1,
  });
}

export function useDiscoverDetail(api: ApiClient, card: DiscoverCard | null) {
  const item = card ? itemOf(card) : null;
  return useQuery({
    queryKey: item ? discoverKeys.detail(item) : ["fork-discover", "detail", "none"],
    queryFn: () => api.call("discoverDetail", item!),
    enabled: item !== null,
    staleTime: 10 * 60_000,
    retry: 1,
  });
}

export function itemOf(card: DiscoverCard): DiscoverItemRequest {
  return { source: card.source, kind: card.kind, id: card.id };
}

export type Prepared = { source: FileSource; preview: ImportPreview_Serialize };

/** Downloads a hub item into the app cache and reads it with the regular import preview. */
export async function prepareImport(api: ApiClient, card: DiscoverCard): Promise<Prepared> {
  const download = await api.call("discoverDownload", itemOf(card));
  const preview = await api.call("importPreview", { source: download.source });
  return { source: download.source, preview };
}

export function useDebounced<T>(value: T, delay = 400): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);
  return debounced;
}

const PREFS_KEY = "lettuce.fork.discover";

export interface Prefs {
  source: DiscoverSource;
  nsfw: boolean;
  nsfl: boolean;
}

const KNOWN_SOURCES: readonly DiscoverSource[] = ["chub", "character_tavern", "wyvern", "botbooru", "illarin"];

export function loadPrefs(): Prefs {
  try {
    const raw = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") as Partial<Prefs>;
    // Sources that were dropped from the fork fall back to Chub.
    const source = raw.source && KNOWN_SOURCES.includes(raw.source) ? raw.source : "chub";
    return { source, nsfw: raw.nsfw ?? true, nsfl: raw.nsfl === true };
  } catch {
    return { source: "chub", nsfw: true, nsfl: false };
  }
}

export function savePrefs(prefs: Prefs) {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // Private mode or full storage: preferences just don't persist.
  }
}
