/**
 * Fork-only: the prompt preview after SillyTavern macros. Entries are
 * evaluated in the order the model reads them (before the chat, then the
 * in-chat ones deepest first) and nothing is saved.
 */
import { useMemo } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import type { MacroPreviewResult, PromptPreview } from "@/api/generated/bindings";
import { useApi } from "@/api/use-api";
import { macroQueryKeys } from "./queries";

function ordered(preview: PromptPreview): number[] {
  const indexes = preview.entries.map((_, index) => index);
  const before = indexes.filter((index) => preview.entries[index]!.position === "relative");
  const inside = indexes
    .filter((index) => preview.entries[index]!.position !== "relative")
    .toSorted((a, b) => preview.entries[b]!.depth - preview.entries[a]!.depth);
  return [...before, ...inside];
}

export function useMacroPreview(
  preview: PromptPreview | null,
  names: { user: string | null; char: string | null },
  conversationId: string | null = null,
): { shown: PromptPreview | null; result: MacroPreviewResult | null } {
  const api = useApi();
  const order = useMemo(() => (preview ? ordered(preview) : []), [preview]);
  const texts = useMemo(() => (preview ? order.map((index) => preview.entries[index]!.text) : []), [preview, order]);
  const needed = texts.some((text) => text.includes("{{"));
  const query = useQuery({
    queryKey: macroQueryKeys.preview(texts, conversationId, names.user, names.char),
    queryFn: () =>
      api.call("macroPreview", { conversation_id: conversationId, texts, user_name: names.user, character_name: names.char }),
    enabled: needed,
    placeholderData: keepPreviousData,
    retry: false,
  });
  const result = needed ? (query.data ?? null) : null;
  const shown = useMemo(() => {
    if (!preview || !result || result.texts.length !== order.length) return preview;
    const entries = [...preview.entries];
    order.forEach((index, at) => {
      entries[index] = { ...entries[index]!, text: result.texts[at] ?? entries[index]!.text };
    });
    return { ...preview, entries };
  }, [preview, result, order]);
  return { shown, result };
}
