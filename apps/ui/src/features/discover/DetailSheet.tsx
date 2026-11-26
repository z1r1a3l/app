// Fork: one hub result's details, with Import and an external link.
import { BookOpen, Download, ExternalLink, Eye, Heart, MessageSquare, MessagesSquare, Star } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Alert, Button, compactNumber, icon, Sheet, Skeleton } from "@lettuceai/crisp";
import { openExternalUrl } from "@/api/external-links";
import type { ApiClient } from "@/api/client";
import type { DiscoverCard, DiscoverStats } from "@/api/generated/bindings";
import { useDiscoverDetail } from "./data";
import { DiscoverError } from "./errors";
import { sourceInfo } from "./sources";
import { Cover } from "./Tile";

function Stat({ value, label, children }: { value: number | null; label: string; children: ReactNode }) {
  if (value == null) return null;
  return (
    <span className="inline-flex items-center gap-1 text-xs text-fg-2" title={label} aria-label={`${label}: ${value}`}>
      {children}
      {compactNumber(value)}
    </span>
  );
}

type StatKey = "downloads" | "stars" | "likes" | "chats" | "messages" | "views";

function Stats({ stats, labels }: { stats: DiscoverStats; labels: (key: StatKey) => string }) {
  return (
    <div className="flex flex-wrap gap-x-3 gap-y-1">
      <Stat value={stats.downloads} label={labels("downloads")}><Download size={icon.sm} /></Stat>
      <Stat value={stats.stars} label={labels("stars")}><Star size={icon.sm} /></Stat>
      <Stat value={stats.likes} label={labels("likes")}><Heart size={icon.sm} /></Stat>
      <Stat value={stats.chats} label={labels("chats")}><MessagesSquare size={icon.sm} /></Stat>
      <Stat value={stats.messages} label={labels("messages")}><MessageSquare size={icon.sm} /></Stat>
      <Stat value={stats.views} label={labels("views")}><Eye size={icon.sm} /></Stat>
    </div>
  );
}

export function DetailSheet({
  api,
  card,
  importing,
  onImport,
  onClose,
}: {
  api: ApiClient;
  card: DiscoverCard | null;
  importing: boolean;
  onImport: (card: DiscoverCard) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation("home");
  const detail = useDiscoverDetail(api, card);
  const shown = detail.data?.card ?? card;
  const site = card ? sourceInfo(card.source) : null;
  const blocker = detail.data?.import_blocker ?? null;
  const description = detail.data?.description || card?.tagline || "";
  return (
    <Sheet
      open={card !== null}
      onClose={onClose}
      size="lg"
      title={shown?.name}
      description={site ? `${site.name}${shown?.creator ? ` · @${shown.creator}` : ""}` : undefined}
      footer={
        card && (
          <div className="flex gap-2">
            {shown?.page_url && (
              <Button variant="secondary" leading={<ExternalLink size={icon.md} />} onClick={() => void openExternalUrl(shown.page_url!)}>
                {t("discover.openSite")}
              </Button>
            )}
            <Button
              variant="primary"
              className="flex-1"
              leading={<Download size={icon.md} />}
              loading={importing}
              disabled={blocker !== null}
              onClick={() => onImport(card)}
            >
              {t(`discover.import.${card.kind}`)}
            </Button>
          </div>
        )
      }
    >
      {shown && (
        <div className="space-y-4 pb-2">
          <div className="flex gap-3">
            <Cover card={shown} className="h-36 w-[6.75rem] shrink-0 rounded-xl" />
            <div className="flex min-w-0 flex-1 flex-col gap-2">
              <Stats stats={shown.stats} labels={(key) => t(`discover.stat.${key}`)} />
              <div className="flex flex-wrap gap-1">
                {shown.nsfw && <span className="rounded-full bg-danger/15 px-2 py-0.5 text-[11px] font-semibold text-danger">{t("discover.adult")}</span>}
                {shown.tokens != null && (
                  <span className="rounded-full bg-fill px-2 py-0.5 text-[11px] text-fg-2">{t("discover.tokens", { count: shown.tokens })}</span>
                )}
                {detail.data?.has_lorebook && (
                  <span className="inline-flex items-center gap-1 rounded-full bg-accent/15 px-2 py-0.5 text-[11px] text-accent">
                    <BookOpen size={11} />
                    {t("discover.hasLorebook")}
                  </span>
                )}
                {(detail.data?.alternate_greetings ?? 0) > 0 && (
                  <span className="rounded-full bg-fill px-2 py-0.5 text-[11px] text-fg-2">
                    {t("discover.altGreetings", { count: detail.data!.alternate_greetings })}
                  </span>
                )}
              </div>
              {shown.tags.length > 0 && (
                <div className="flex flex-wrap gap-1">
                  {shown.tags.slice(0, 16).map((tag) => (
                    <span key={tag} className="rounded-full border border-line px-2 py-0.5 text-[11px] text-fg-2">{tag}</span>
                  ))}
                </div>
              )}
            </div>
          </div>
          {blocker && <Alert tone="warning" title={t("discover.cannotImport")} description={blocker} />}
          {detail.isError && <DiscoverError error={detail.error} site={site?.name ?? ""} onRetry={() => void detail.refetch()} />}
          {detail.isPending ? (
            <div className="space-y-2" role="status" aria-label={t("discover.loading")}>
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-5/6" />
              <Skeleton className="h-4 w-2/3" />
            </div>
          ) : (
            <>
              {description && (
                <section className="space-y-1">
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-fg-3">{t("discover.about")}</h3>
                  <p className="max-h-72 overflow-y-auto whitespace-pre-wrap text-sm leading-relaxed text-fg-2">{description}</p>
                </section>
              )}
              {detail.data?.greeting && (
                <section className="space-y-1">
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-fg-3">{t("discover.greeting")}</h3>
                  <p className="max-h-72 overflow-y-auto whitespace-pre-wrap rounded-xl bg-fill p-3 text-sm leading-relaxed text-fg-2">
                    {detail.data.greeting}
                  </p>
                </section>
              )}
            </>
          )}
          {site && <p className="text-[11px] text-fg-3">{t("discover.sources.credit", { url: site.url })}</p>}
        </div>
      )}
    </Sheet>
  );
}
