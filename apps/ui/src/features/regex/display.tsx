/**
 * Fork-only: display-side regex for a chat. The timeline provides each
 * message's depth; rows ask for their display text through
 * `useRegexDisplay`.
 */
import { useMemo, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { useApi } from "@/api/use-api";
import { RegexDisplayContext, type RegexDisplay } from "./display-context";
import { regexQueryKeys } from "./queries";
import { varRecord } from "@/features/macros";


export function RegexDisplayProvider({
  conversationId,
  messageIds,
  children,
}: {
  conversationId: string;
  /** Visible messages oldest first (user and assistant only). */
  messageIds: readonly string[];
  children: ReactNode;
}) {
  const api = useApi();
  const scripts = useQuery({
    queryKey: regexQueryKeys.conversation(conversationId),
    queryFn: () => api.call("regexConversationScripts", { conversation_id: conversationId }),
    staleTime: 30_000,
  });
  const depths = useMemo(() => {
    const map = new Map<string, number>();
    messageIds.forEach((id, index) => map.set(id, messageIds.length - index - 1));
    return map;
  }, [messageIds]);
  const value = useMemo<RegexDisplay | null>(() => {
    const data = scripts.data;
    if (!data || data.scripts.length === 0) return null;
    return {
      scripts: data.scripts,
      macros: { user: data.user_name, char: data.character_name, vars: varRecord(data.chat_vars), globalVars: varRecord(data.global_vars) },
      depthOf: (id) => depths.get(id),
    };
  }, [scripts.data, depths]);
  return <RegexDisplayContext.Provider value={value}>{children}</RegexDisplayContext.Provider>;
}
