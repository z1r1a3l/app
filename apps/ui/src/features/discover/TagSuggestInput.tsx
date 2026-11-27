// Fork: the Discover tag field with suggestions. Hubs that publish tags
// (Chub's full list, Illarin's tag search) suggest from the hub; the others
// suggest tags seen on the results shown so far.
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { compactNumber, icon } from "@lettuceai/crisp";
import type { ApiClient } from "@/api/client";
import type { DiscoverSource, DiscoverTag } from "@/api/generated/bindings";
import { useDebounced } from "./data";

const MAX_SHOWN = 12;

function matchTags(tags: readonly DiscoverTag[], prefix: string, taken: readonly string[]): DiscoverTag[] {
  const needle = prefix.trim().toLowerCase();
  const used = new Set(taken.map((tag) => tag.toLowerCase()));
  const free = tags.filter((tag) => !used.has(tag.name.toLowerCase()));
  const starts = free.filter((tag) => tag.name.toLowerCase().startsWith(needle));
  const contains = needle ? free.filter((tag) => !tag.name.toLowerCase().startsWith(needle) && tag.name.toLowerCase().includes(needle)) : [];
  return [...starts, ...contains].slice(0, MAX_SHOWN);
}

export function TagSuggestInput({
  api,
  source,
  seen,
  value,
  onChange,
  max,
  placeholder,
}: {
  api: ApiClient;
  source: DiscoverSource;
  /** Tags on the results shown so far, for hubs without a tag list. */
  seen: readonly string[];
  value: string[];
  onChange: (next: string[]) => void;
  max: number;
  placeholder: string;
}) {
  const { t } = useTranslation("home");
  const [text, setText] = useState("");
  const [focused, setFocused] = useState(false);
  const prefix = useDebounced(text.trim());
  const remote = useQuery({
    queryKey: ["fork-discover", "tags", source, source === "chub" ? "" : prefix.toLowerCase()],
    queryFn: () => api.call("discoverTags", { source, prefix: source === "chub" ? "" : prefix }),
    enabled: focused && (source === "chub" || prefix.length > 0),
    staleTime: source === "chub" ? Number.POSITIVE_INFINITY : 10 * 60_000,
    gcTime: 24 * 60 * 60_000,
    retry: 0,
  });
  const fromResults: DiscoverTag[] = [...new Set(seen)].map((name) => ({ name, count: null }));
  const pool = remote.data && !remote.data.unsupported && remote.data.tags.length > 0 ? remote.data.tags : fromResults;
  const suggestions = focused && value.length < max ? matchTags(pool, text, value) : [];
  const add = (tag: string) => {
    const name = tag.trim();
    if (!name || value.length >= max || value.some((item) => item.toLowerCase() === name.toLowerCase())) return;
    onChange([...value, name]);
    setText("");
  };
  return (
    <div className="space-y-1.5">
      <div className="flex min-h-11 flex-wrap items-center gap-1.5 rounded-xl border border-line bg-fill px-2 py-1.5 focus-within:border-accent">
        {value.map((tag) => (
          <span key={tag} className="inline-flex items-center gap-1 rounded-full bg-fill-3 py-0.5 pl-2.5 pr-1 text-sm text-fg">
            {tag}
            <button type="button" aria-label={t("discover.removeTag", { tag })} className="rounded-full p-0.5 text-fg-3 hover:text-fg" onClick={() => onChange(value.filter((item) => item !== tag))}>
              <X size={icon.sm} />
            </button>
          </span>
        ))}
        <input
          value={text}
          disabled={value.length >= max}
          placeholder={value.length === 0 ? placeholder : ""}
          aria-label={placeholder}
          className="min-w-24 flex-1 bg-transparent py-1 text-base text-fg outline-none placeholder:text-fg-4"
          onFocus={() => setFocused(true)}
          onBlur={() => setTimeout(() => setFocused(false), 150)}
          onChange={(event) => {
            const next = event.target.value;
            if (next.endsWith(",")) add(next.slice(0, -1));
            else setText(next);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              add(text);
            } else if (event.key === "Backspace" && !text && value.length > 0) {
              onChange(value.slice(0, -1));
            }
          }}
        />
      </div>
      {suggestions.length > 0 && (
        <div role="listbox" aria-label={t("discover.tagSuggestions")} className="flex flex-wrap gap-1.5">
          {suggestions.map((tag) => (
            <button
              key={tag.name}
              type="button"
              role="option"
              aria-selected={false}
              className="inline-flex items-center gap-1 rounded-full border border-line px-2.5 py-1 text-sm text-fg-2 hover:bg-fill-2"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => add(tag.name)}
            >
              {tag.name}
              {tag.count != null && <span className="text-xs text-fg-4">{compactNumber(tag.count)}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
