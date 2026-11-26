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
    enabled: item !== null && card?.source !== "janny",
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
}

export function loadPrefs(): Prefs {
  try {
    const raw = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") as Partial<Prefs>;
    return { source: raw.source ?? "chub", nsfw: raw.nsfw ?? true };
  } catch {
    return { source: "chub", nsfw: true };
  }
}

export function savePrefs(prefs: Prefs) {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // Private mode or full storage: preferences just don't persist.
  }
}
