// Fork: Discover browses public character-card hubs (Chub, Character
// Tavern, Wyvern, Botbooru, Illarin) through the Rust
// side, so no hub page, script or ad is ever loaded. Import hands the
// downloaded file to the regular import sheet.
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { useRouteContext } from "@tanstack/react-router";
import { Compass, Globe, SlidersHorizontal } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Button,
  EmptyState,
  icon,
  IconButton,
  InfiniteScroll,
  Page,
  Segmented,
  Select,
  Sheet,
  Switch,
  toast,
} from "@lettuceai/crisp";
import type { DiscoverCard, DiscoverKind, DiscoverSource } from "@/api/generated/bindings";
import { ImportSheet } from "@/features/opened-files";
import { MobileSettingsButton } from "@/shared/ui/MobileSettingsButton";
import { type Filters, loadPrefs, type Prepared, prepareImport, savePrefs, useDebounced, useDiscoverSearch } from "./data";
import { DetailSheet } from "./DetailSheet";
import { DiscoverError } from "./errors";
import { discoverErrorKey } from "./format";
import { PresetImportSheet } from "./PresetImportSheet";
import { type Section, sortsFor, sourceInfo, sourcesFor } from "./sources";
import { SourcesSheet } from "./SourcesSheet";
import { Tile, TileSkeleton } from "./Tile";
import { TagSuggestInput } from "./TagSuggestInput";

const SHEET_KINDS = new Set(["character", "persona", "lorebook"]);

export function DiscoverPage() {
  const { t } = useTranslation("home");
  const api = useRouteContext({ from: "__root__", select: (context) => context.api });
  const [prefs] = useState(loadPrefs);
  const [section, setSection] = useState<Section>("characters");
  const [resourceKind, setResourceKind] = useState<DiscoverKind>("lorebook");
  const kind: DiscoverKind = section === "characters" ? "character" : resourceKind;
  const [chosen, setChosen] = useState<DiscoverSource>(prefs.source);
  const available = sourcesFor(kind);
  const source = available.some((item) => item.id === chosen) ? chosen : available[0]!.id;
  const info = sourceInfo(source);
  const sorts = sortsFor(info, kind);
  const [sortBySource, setSortBySource] = useState<Partial<Record<string, string>>>({});
  const sort = sortBySource[`${source}:${kind}`] ?? sorts[0]?.value ?? "";
  const [query, setQuery] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  const [excludeTags, setExcludeTags] = useState<string[]>([]);
  const [nsfw, setNsfw] = useState(prefs.nsfw);
  const [nsfl, setNsfl] = useState(prefs.nsfl);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [selected, setSelected] = useState<DiscoverCard | null>(null);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const debouncedQuery = useDebounced(query.trim());

  const filters: Filters = { source, kind, query: debouncedQuery, tags, excludeTags: info.noExclude ? [] : excludeTags, sort: sort || null, nsfw, nsfl: Boolean(info.nsfl) && nsfw && nsfl };
  const results = useDiscoverSearch(api, filters);
  const seen = new Set<string>();
  const items = (results.data?.pages ?? []).flatMap((page) => page.items).filter((card) => !seen.has(card.id) && seen.add(card.id));
  const seenTags = items.flatMap((card) => card.tags);
  const total = results.data?.pages[0]?.total ?? null;
  const nextPageError = results.isFetchNextPageError ? results.error : null;

  const pickSource = (next: DiscoverSource) => {
    setChosen(next);
    savePrefs({ source: next, nsfw, nsfl });
  };
  const toggleNsfw = (next: boolean) => {
    setNsfw(next);
    savePrefs({ source: chosen, nsfw: next, nsfl });
  };
  const toggleNsfl = (next: boolean) => {
    setNsfl(next);
    savePrefs({ source: chosen, nsfw, nsfl: next });
  };

  const importing = useMutation({
    mutationFn: (card: DiscoverCard) => prepareImport(api, card),
    onSuccess: (result) => {
      setSelected(null);
      if (SHEET_KINDS.has(result.preview.content.kind) || result.preview.content.kind === "prompt") setPrepared(result);
      else toast.error(t("discover.error.notImportable"));
    },
    onError: (error) => toast.error(t(discoverErrorKey(error), { site: info.name })),
  });

  const activeFilters = tags.length + (info.noExclude ? 0 : excludeTags.length);

  const body = results.isPending ? (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5" role="status" aria-label={t("discover.loading")}>
      {Array.from({ length: 6 }, (_, index) => <TileSkeleton key={index} />)}
    </div>
  ) : results.isError ? (
    <DiscoverError error={results.error} site={info.name} onRetry={() => void results.refetch()} />
  ) : items.length === 0 ? (
    <EmptyState icon={<Compass size={icon.xl} />} title={t("discover.emptyTitle")} description={t("discover.emptyBody", { site: info.name })} className="pt-10" />
  ) : (
    <>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
        {items.map((card) => <Tile key={`${card.source}:${card.id}`} card={card} onOpen={() => setSelected(card)} />)}
      </div>
      {nextPageError && <DiscoverError error={nextPageError} site={info.name} onRetry={() => void results.fetchNextPage()} />}
      <InfiniteScroll
        onLoadMore={() => void results.fetchNextPage()}
        disabled={!results.hasNextPage || results.isFetchNextPageError}
        loading={results.isFetchingNextPage}
        endMessage={t("discover.end")}
        rootMargin="600px"
      />
    </>
  );

  return (
    <>
      <Page
        title={t("discover.title")}
        meta={total != null ? t("discover.total", { count: total, site: info.name }) : info.name}
        searchValue={query}
        onSearchChange={setQuery}
        searchPlaceholder={t("discover.searchPlaceholder", { site: info.name })}
        contentClassName="space-y-3 pb-[calc(1.5rem+var(--safe-bottom,0px))]"
        actions={
          <div className="flex items-center gap-1">
            <IconButton label={t("discover.sources.open")} onClick={() => setSourcesOpen(true)}>
              <Globe size={icon.lg} />
            </IconButton>
            <MobileSettingsButton />
          </div>
        }
        filters={
          <div className="space-y-2">
            <Segmented
              block
              size="sm"
              aria-label={t("discover.section.label")}
              value={section}
              onChange={setSection}
              options={[
                { value: "characters", label: t("discover.section.characters") },
                { value: "resources", label: t("discover.section.resources") },
              ]}
            />
            {section === "resources" && (
              <Segmented
                block
                size="sm"
                aria-label={t("discover.resourceKind.label")}
                value={resourceKind}
                onChange={setResourceKind}
                options={[
                  { value: "lorebook", label: t("discover.resourceKind.lorebook") },
                  { value: "preset", label: t("discover.resourceKind.preset") },
                ]}
              />
            )}
            <div className="-mx-4 flex gap-1.5 overflow-x-auto px-4 pb-0.5 [scrollbar-width:none]" role="tablist" aria-label={t("discover.sources.title")}>
              {available.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  role="tab"
                  aria-selected={item.id === source}
                  onClick={() => pickSource(item.id)}
                  className={`shrink-0 rounded-full border px-3 py-1 text-sm transition-colors ${item.id === source ? "border-accent bg-accent text-on-accent" : "border-line bg-surface-1 text-fg-2 hover:bg-fill"}`}
                >
                  {item.name}
                </button>
              ))}
            </div>
          </div>
        }
      >
        <div className="flex items-center gap-2">
          {sorts.length > 1 && (
            <Select
              aria-label={t("discover.sortLabel")}
              className="min-w-0 flex-1"
              value={sort}
              onChange={(event) => setSortBySource((current) => ({ ...current, [`${source}:${kind}`]: event.target.value }))}
              options={sorts.map((option) => ({ value: option.value, label: t(`discover.sort.${option.label}`) }))}
            />
          )}
          <Button
            variant={activeFilters > 0 ? "tonal" : "secondary"}
            size="sm"
            leading={<SlidersHorizontal size={icon.sm} />}
            onClick={() => setFiltersOpen(true)}
          >
            {activeFilters > 0 ? t("discover.tagsCount", { count: activeFilters }) : t("discover.tags")}
          </Button>
          <label className="flex shrink-0 items-center gap-1.5 text-sm text-fg-2">
            <span>{t("discover.adult")}</span>
            <Switch checked={nsfw} onChange={toggleNsfw} aria-label={t("discover.nsfw")} />
          </label>
        </div>
        {body}
      </Page>
      <Sheet open={filtersOpen} onClose={() => setFiltersOpen(false)} title={t("discover.filtersTitle")} description={info.name}>
        <div className="space-y-4 pb-2">
          <div className="space-y-1.5">
            <div className="text-sm font-medium">{t("discover.includeTags")}</div>
            <TagSuggestInput api={api} source={source} seen={seenTags} value={tags} onChange={setTags} max={6} placeholder={t("discover.tagPlaceholder")} />
          </div>
          {!info.noExclude && (
            <div className="space-y-1.5">
              <div className="text-sm font-medium">{t("discover.excludeTags")}</div>
              <TagSuggestInput api={api} source={source} seen={seenTags} value={excludeTags} onChange={setExcludeTags} max={6} placeholder={t("discover.tagPlaceholder")} />
            </div>
          )}
          <label className="flex items-center justify-between gap-3 rounded-xl bg-fill px-3 py-2.5">
            <span className="text-sm">
              <span className="block font-medium">{t("discover.nsfw")}</span>
              <span className="block text-xs text-fg-3">{t("discover.nsfwHint")}</span>
            </span>
            <Switch checked={nsfw} onChange={toggleNsfw} aria-label={t("discover.nsfw")} />
          </label>
          {info.nsfl && (
            <label className="flex items-center justify-between gap-3 rounded-xl bg-fill px-3 py-2.5">
              <span className="text-sm">
                <span className={`block font-medium ${nsfw ? "" : "text-fg-3"}`}>{t("discover.nsfl")}</span>
                <span className="block text-xs text-fg-3">{t(nsfw ? "discover.nsflHint" : "discover.nsflNeedsAdult")}</span>
              </span>
              <Switch checked={nsfw && nsfl} onChange={toggleNsfl} disabled={!nsfw} aria-label={t("discover.nsfl")} />
            </label>
          )}
          {activeFilters > 0 && (
            <Button variant="ghost" onClick={() => { setTags([]); setExcludeTags([]); }}>
              {t("discover.clearTags")}
            </Button>
          )}
        </div>
      </Sheet>
      <SourcesSheet open={sourcesOpen} kind={kind} current={source} onPick={pickSource} onClose={() => setSourcesOpen(false)} />
      <DetailSheet
        api={api}
        card={selected}
        importing={importing.isPending}
        onImport={(card) => importing.mutate(card)}
        onClose={() => !importing.isPending && setSelected(null)}
        onTag={(tag) => {
          setTags([tag]);
          setQuery("");
          setSelected(null);
        }}
      />
      {prepared && prepared.preview.content.kind === "prompt" && (
        <PresetImportSheet api={api} source={prepared.source} preview={prepared.preview} onClose={() => setPrepared(null)} />
      )}
      {prepared && prepared.preview.content.kind !== "prompt" && (
        <ImportSheet api={api} source={prepared.source} preview={prepared.preview} onClose={() => setPrepared(null)} />
      )}
    </>
  );
}
