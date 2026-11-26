// Fork: one hub result in the Discover grid.
import { BookOpen, FileText, User } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { compactNumber, icon } from "@lettuceai/crisp";
import type { DiscoverCard } from "@/api/generated/bindings";
import { headlineStat } from "./format";

export function KindIcon({ kind, size }: { kind: DiscoverCard["kind"]; size: number }) {
  if (kind === "lorebook") return <BookOpen size={size} />;
  if (kind === "preset") return <FileText size={size} />;
  return <User size={size} />;
}

export function Cover({ card, className }: { card: DiscoverCard; className?: string }) {
  const [failed, setFailed] = useState(false);
  if (!card.avatar_url || failed)
    return (
      <div className={`flex items-center justify-center bg-surface-2 text-fg-3 ${className ?? ""}`}>
        <KindIcon kind={card.kind} size={icon.xl} />
      </div>
    );
  return (
    <img
      src={card.avatar_url}
      alt=""
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      onError={() => setFailed(true)}
      className={`object-cover ${className ?? ""}`}
    />
  );
}

export function Tile({ card, onOpen }: { card: DiscoverCard; onOpen: () => void }) {
  const { t } = useTranslation("home");
  const stat = headlineStat(card);
  return (
    <button
      type="button"
      onClick={onOpen}
      className="group flex min-w-0 flex-col overflow-hidden rounded-2xl border border-line bg-surface-1 text-left transition-colors hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-accent"
    >
      <div className="relative aspect-[3/4] w-full overflow-hidden bg-surface-2">
        <Cover card={card} className="absolute inset-0 h-full w-full" />
        <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 via-black/40 to-transparent px-2.5 pb-2 pt-8">
          <div className="line-clamp-2 text-sm font-semibold leading-tight text-white">{card.name}</div>
          {card.creator && <div className="truncate text-xs text-white/70">{t("discover.creator", { name: card.creator })}</div>}
        </div>
        <div className="absolute left-2 top-2 flex gap-1">
          {card.nsfw && <span className="rounded-md bg-danger/90 px-1.5 py-0.5 text-[10px] font-semibold text-white">{t("discover.adult")}</span>}
          {stat && <span className="rounded-md bg-black/55 px-1.5 py-0.5 text-[10px] font-medium text-white">{stat}</span>}
        </div>
        {card.tokens != null && (
          <span className="absolute right-2 top-2 rounded-md bg-black/55 px-1.5 py-0.5 text-[10px] text-white">
            {t("discover.tokensShort", { count: compactNumber(card.tokens) })}
          </span>
        )}
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-1.5 p-2.5">
        {card.tagline && <p className="line-clamp-2 text-xs leading-snug text-fg-3">{card.tagline}</p>}
        {card.tags.length > 0 && (
          <div className="mt-auto flex h-[1.35rem] flex-wrap gap-1 overflow-hidden">
            {card.tags.slice(0, 4).map((tag) => (
              <span key={tag} className="max-w-full truncate rounded-full bg-fill px-2 py-0.5 text-[10px] text-fg-3">{tag}</span>
            ))}
          </div>
        )}
      </div>
    </button>
  );
}

export function TileSkeleton() {
  return (
    <div className="flex flex-col overflow-hidden rounded-2xl border border-line bg-surface-1">
      <div className="aspect-[3/4] w-full animate-pulse bg-surface-2" />
      <div className="space-y-1.5 p-2.5">
        <div className="h-3 w-4/5 animate-pulse rounded bg-surface-2" />
        <div className="h-3 w-3/5 animate-pulse rounded bg-surface-2" />
      </div>
    </div>
  );
}
