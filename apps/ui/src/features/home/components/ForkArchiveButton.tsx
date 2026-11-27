// Fork: an "Archive" entry on the Chats tab. Archived chats of every
// character and group, with open and restore, so archiving never hides a
// chat behind the per-character chat history filter.
import { useState } from "react";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { Archive, ArchiveRestore } from "lucide-react";
import { useTranslation } from "react-i18next";
import { EmptyState, icon, IconButton, LoadingState, Sheet, toast } from "@lettuceai/crisp";
import type { ApiClient } from "@/api/client";
import type { CharacterSummary, ConversationSummary } from "@/api/generated/bindings";
import { queryKeys } from "@/api/query-keys";
import { FailureState } from "@/shared/ui/FailureState";
import { RecentRow } from "./RecentList";

function archivedChatsQuery(api: ApiClient) {
  return {
    queryKey: [...queryKeys.conversations.lists(), "fork-archived"] as const,
    queryFn: ({ pageParam }: { pageParam: string | null }) =>
      api.call("conversationsList", { kind: null, character_id: null, source_group_id: null, lifecycle: "archived", cursor: pageParam, limit: null }),
    initialPageParam: null as string | null,
    getNextPageParam: (page: { next_cursor: string | null }) => page.next_cursor,
  };
}

export function ForkArchiveButton({ api, characters, onOpen }: { api: ApiClient; characters: ReadonlyMap<string, CharacterSummary>; onOpen: (chat: ConversationSummary) => void }) {
  const { t } = useTranslation("fork-ui");
  const [open, setOpen] = useState(false);
  return (
    <>
      <IconButton label={t("archive.open")} onClick={() => setOpen(true)}>
        <Archive size={icon.lg} />
      </IconButton>
      <Sheet open={open} onClose={() => setOpen(false)} size="md" title={t("archive.title")}>
        {open && <ArchivedList api={api} characters={characters} onOpen={(chat) => { setOpen(false); onOpen(chat); }} />}
      </Sheet>
    </>
  );
}

function ArchivedList({ api, characters, onOpen }: { api: ApiClient; characters: ReadonlyMap<string, CharacterSummary>; onOpen: (chat: ConversationSummary) => void }) {
  const { t } = useTranslation("fork-ui");
  const queryClient = useQueryClient();
  const chats = useInfiniteQuery(archivedChatsQuery(api));
  const [busy, setBusy] = useState<string | null>(null);
  if (chats.isPending) return <LoadingState />;
  if (chats.isError) return <FailureState error={chats.error} onRetry={() => void chats.refetch()} />;
  const all = chats.data.pages.flatMap((page) => page.items).sort((a, b) => b.updated_at - a.updated_at);
  if (all.length === 0) return <EmptyState size="sm" icon={<Archive size={icon.xl} />} title={t("archive.empty")} />;
  const characterOf = (chat: ConversationSummary) => (chat.source.type === "direct" ? (characters.get(chat.source.character_id) ?? null) : null);
  const restore = async (chat: ConversationSummary) => {
    setBusy(chat.id);
    try {
      await api.call("conversationRestore", { conversation_id: chat.id });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.conversations.lists() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.home.characters() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.conversations.detail(chat.id) }),
      ]);
      toast.success(t("archive.restored"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="space-y-1 pb-2">
      {all.map((chat) => (
        <div key={chat.id} className="flex items-center gap-1">
          <div className="min-w-0 flex-1">
            <RecentRow chat={chat} character={characterOf(chat)} held={false} onOpen={() => onOpen(chat)} onMenu={() => undefined} />
          </div>
          <IconButton label={t("archive.restore", { title: chat.title || t("archive.untitled") })} disabled={busy !== null} onClick={() => void restore(chat)} className="shrink-0">
            <ArchiveRestore size={icon.md} />
          </IconButton>
        </div>
      ))}
      {chats.hasNextPage && (
        <button type="button" className="w-full py-2 text-sm text-fg-3" onClick={() => void chats.fetchNextPage()}>{t("archive.more")}</button>
      )}
    </div>
  );
}
