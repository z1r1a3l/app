// Fork (z1r1a3l): review proposed entry updates; nothing is written until "apply".
import { useTranslation } from "react-i18next";
import { Badge, Button, Checkbox, Collapsible, Diff } from "@lettuceai/crisp";
import type { LorebookEntryView } from "@/api/generated/bindings";
import type { Proposal } from "./data";

export interface Unchanged {
  entry: LorebookEntryView;
  reason: string | null;
}

export interface Failed {
  entry: LorebookEntryView;
  code: string;
}

export function ProposalReview({
  proposals,
  selected,
  unchanged,
  failed,
  busy,
  onToggle,
  onEdit,
  onReject,
}: {
  proposals: readonly Proposal[];
  selected: ReadonlySet<string>;
  unchanged: readonly Unchanged[];
  failed: readonly Failed[];
  busy: boolean;
  onToggle: (entryId: string, on: boolean) => void;
  onEdit: (proposal: Proposal) => void;
  onReject: (entryId: string) => void;
}) {
  const { t } = useTranslation("fork-lorebook");
  const { t: creation } = useTranslation("lorebook-create");
  return (
    <div className="space-y-4 pb-1">
      {proposals.length === 0 ? (
        <p className="text-sm text-fg-3">{t("review.nothing")}</p>
      ) : (
        <p className="text-sm text-fg-3">{t("review.intro", { count: proposals.length })}</p>
      )}
      {proposals.map((proposal) => {
        const id = proposal.entry.id;
        const keywordsChanged = proposal.keywords.join("\n") !== proposal.entry.keywords.join("\n");
        const titleChanged = proposal.title !== proposal.entry.title;
        return (
          <div key={id} className="space-y-3 rounded-2xl border border-line bg-surface-1 p-3">
            <div className="flex items-start gap-2">
              <Checkbox
                checked={selected.has(id)}
                disabled={busy}
                onChange={(on) => onToggle(id, on)}
                aria-label={t("review.select", { title: proposal.entry.title || t("untitled") })}
              />
              <div className="min-w-0 flex-1">
                <p className="truncate text-base font-medium text-fg">{proposal.entry.title || t("untitled")}</p>
                {titleChanged && <p className="text-sm text-fg-3">{t("review.newTitle", { title: proposal.title })}</p>}
              </div>
            </div>
            <Diff
              before={proposal.entry.content}
              after={proposal.content}
              beforeLabel={t("review.before")}
              afterLabel={t("review.after")}
              view="unified"
              context={2}
            />
            {keywordsChanged && (
              <div className="space-y-1 text-sm">
                <p className="text-fg-3">{t("review.keywords")}</p>
                <div className="flex flex-wrap gap-1">
                  {proposal.keywords.map((keyword) => (
                    <Badge key={keyword} tone={proposal.entry.keywords.includes(keyword) ? "neutral" : "accent"}>
                      {keyword}
                    </Badge>
                  ))}
                </div>
              </div>
            )}
            <div className="flex justify-end gap-2">
              <Button variant="ghost" size="sm" disabled={busy} onClick={() => onReject(id)}>
                {t("review.reject")}
              </Button>
              <Button variant="secondary" size="sm" disabled={busy} onClick={() => onEdit(proposal)}>
                {t("review.edit")}
              </Button>
            </div>
          </div>
        );
      })}
      {unchanged.length > 0 && (
        <Collapsible variant="plain" title={t("review.unchanged", { count: unchanged.length })}>
          <ul className="space-y-1 text-sm">
            {unchanged.map((item) => (
              <li key={item.entry.id} className="text-fg-2">
                <span className="font-medium">{item.entry.title || t("untitled")}</span>
                {item.reason ? <span className="text-fg-3">{t("review.reasonSuffix", { reason: item.reason })}</span> : null}
              </li>
            ))}
          </ul>
        </Collapsible>
      )}
      {failed.length > 0 && (
        <Collapsible variant="plain" title={t("review.failed", { count: failed.length })}>
          <ul className="space-y-1 text-sm">
            {failed.map((item) => (
              <li key={item.entry.id} className="text-danger">
                <span className="font-medium">{item.entry.title || t("untitled")}</span>
                <span>{t("review.reasonSuffix", { reason: creation(`jobErrors.${item.code}`, { defaultValue: item.code }) })}</span>
              </li>
            ))}
          </ul>
        </Collapsible>
      )}
    </div>
  );
}
