/** Fork-only: helper model (trackers, Notebook) data. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type {
  HelperChatView,
  HelperConfigUpdate,
  HelperScopeUpdate,
  HelperStateDto,
  HelperStatusDto,
} from "@/api/generated/bindings";
import { useApi } from "@/api/use-api";
import { helperQueryKeys } from "./queries";

export const isActive = (status: HelperStatusDto | null | undefined) =>
  status?.state === "queued" || status?.state === "running";

export function useHelperChat(conversationId: string, enabled = true) {
  const api = useApi();
  return useQuery({
    queryKey: helperQueryKeys.chat(conversationId),
    queryFn: () => api.call("helperChatGet", { conversation_id: conversationId }),
    enabled,
  });
}

/** The latest run; polled while the helper is on for the chat. */
export function useHelperStatus(conversationId: string, enabled: boolean) {
  const api = useApi();
  return useQuery({
    queryKey: helperQueryKeys.status(conversationId),
    queryFn: () => api.call("helperStatus", { conversation_id: conversationId }),
    enabled,
    refetchInterval: (query) => (isActive(query.state.data) ? 1000 : 3000),
  });
}

export function useHelperMessage(conversationId: string, messageId: string, variant: string, enabled: boolean) {
  const api = useApi();
  return useQuery({
    queryKey: helperQueryKeys.message(conversationId, messageId, variant),
    queryFn: () => api.call("helperMessage", { conversation_id: conversationId, message_id: messageId }),
    enabled,
  });
}

export function useHelperConfig(enabled = true) {
  const api = useApi();
  return useQuery({ queryKey: helperQueryKeys.config(), queryFn: () => api.call("helperConfigGet"), enabled });
}

export function useHelperConfigWrite() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: (update: HelperConfigUpdate) => api.call("helperConfigSet", update),
    onSuccess: (view) => client.setQueryData(helperQueryKeys.config(), view),
  });
}

export function useHelperScopeWrite(conversationId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: (update: HelperScopeUpdate) => api.call("helperScopeSet", update),
    onSuccess: () => void client.invalidateQueries({ queryKey: ["fork-helper", "chat"] }),
    onSettled: () => void client.invalidateQueries({ queryKey: helperQueryKeys.chat(conversationId) }),
  });
}

export function useHelperStateWrite(conversationId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: (state: HelperStateDto) => api.call("helperStateSet", { conversation_id: conversationId, state }),
    onSuccess: (view: HelperChatView) => {
      client.setQueryData(helperQueryKeys.chat(conversationId), view);
      void client.invalidateQueries({ queryKey: ["fork-helper", "message", conversationId] });
    },
  });
}

export function useHelperStop() {
  const api = useApi();
  return useMutation({ mutationFn: (jobId: string) => api.call("jobCancel", { job_id: jobId }) });
}
