/**
 * Fork-only: under a reply, the helper's collapsible plaque (what changed,
 * through the safe rich path) and, while it reads this reply, its status
 * with a Stop button.
 */
import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Button, Spinner, Text } from "@lettuceai/crisp";
import type { TimelineMessage } from "@/api/generated/bindings";
import { RichMessageBody } from "@/features/regex";
import { isActive, useHelperChat, useHelperMessage, useHelperStatus, useHelperStop } from "./data";
import { plaqueHtml, type PlaqueLabels } from "./plaque-html";
import { helperQueryKeys } from "./queries";

/** Changes when the shown variant or its text changes (swipe, edit). */
export function variantOf(message: TimelineMessage): string {
  const text = JSON.stringify(message.parts);
  let hash = 0;
  for (let i = 0; i < text.length; i++) hash = (hash * 31 + text.charCodeAt(i)) | 0;
  return `${message.candidate_index ?? "-"}:${text.length}:${hash}`;
}

export function HelperPlaque({ conversationId, message }: { conversationId: string; message: TimelineMessage }) {
  const { t } = useTranslation("helper");
  const client = useQueryClient();
  const chat = useHelperChat(conversationId);
  const on = chat.data?.enabled ?? false;
  const status = useHelperStatus(conversationId, on);
  const mine = status.data?.message_id === message.id ? status.data : null;
  const busy = isActive(mine);
  const variant = variantOf(message);
  const read = useHelperMessage(conversationId, message.id, variant, on && !busy);
  const stop = useHelperStop();

  // A finished read of this reply: refresh its plaque and the chat state.
  const settled = mine && !busy ? `${mine.job_id}:${mine.state}` : null;
  useEffect(() => {
    if (!settled) return;
    void client.invalidateQueries({ queryKey: ["fork-helper", "message", conversationId, message.id] });
    void client.invalidateQueries({ queryKey: helperQueryKeys.chat(conversationId) });
  }, [settled, client, conversationId, message.id]);

  const labels = useMemo<PlaqueLabels>(
    () => ({
      title: t("plaque.title"),
      noChanges: t("plaque.noChanges"),
      trackers: {
        scene: t("scene"),
        characters: t("characters"),
        bonds: t("bonds"),
        inventory: t("inventory"),
        npcs: t("npcs"),
        notebook: t("tracker.notebook"),
      },
      removed: t("plaque.removed"),
      hiddenFrom: t("plaque.hiddenFrom"),
      clue: t("plaque.clue"),
      clueMissing: t("plaque.clueMissing"),
      revealed: t("plaque.revealed"),
      hidden: t("plaque.hidden"),
      truthRemoved: t("plaque.truthRemoved"),
    }),
    [t],
  );
  const html = read.data?.read ? plaqueHtml(read.data.changes, labels) : null;

  if (!on) return null;
  return (
    <div className="mt-2 space-y-1 text-sm" data-testid="helper-plaque">
      {busy && mine && (
        <div className="flex items-center gap-2 rounded-lg bg-fill px-3 py-1.5 text-fg-2">
          <Spinner size="sm" />
          <span className="flex-1">{mine.state === "queued" ? t("queued") : t("running")}</span>
          <Button size="sm" variant="ghost" loading={stop.isPending} onClick={() => stop.mutate(mine.job_id)}>
            {t("stop")}
          </Button>
        </div>
      )}
      {!busy && mine?.state === "failed" && <Text variant="hint" className="text-danger">{t("failed", { error: mine.error ?? "" })}</Text>}
      {!busy && html && (
        <div className="rounded-lg border border-line bg-surface-1 px-3 py-1.5 text-fg-2">
          <RichMessageBody text={html} />
        </div>
      )}
    </div>
  );
}
