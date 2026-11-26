/** Fork-only: SillyTavern macro variables (per chat and global). */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { MacroVarDto, MacroVarsView } from "@/api/generated/bindings";
import { useApi } from "@/api/use-api";
import { macroQueryKeys } from "./queries";

export function useMacroVars(conversationId: string | null, enabled = true) {
  const api = useApi();
  return useQuery({
    queryKey: macroQueryKeys.vars(conversationId),
    queryFn: () => api.call("macroVarsGet", { conversation_id: conversationId }),
    enabled,
  });
}

export function useMacroVarsWrite(conversationId: string | null) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ global, vars }: { global: boolean; vars: MacroVarDto[] }) =>
      api.call("macroVarsReplace", { conversation_id: conversationId, global, vars }),
    onSuccess: (view: MacroVarsView) => {
      client.setQueryData(macroQueryKeys.vars(conversationId), view);
      // Display regex reads variables (e.g. a HUD theme), and other chats see new globals.
      void client.invalidateQueries({ queryKey: ["fork-regex", "conversation"] });
      void client.invalidateQueries({ queryKey: ["fork-macros", "vars"] });
      void client.invalidateQueries({ queryKey: ["fork-macros", "preview"] });
    },
  });
}

/** `[{name, value}]` → `{name: value}`. */
export function varRecord(vars: readonly MacroVarDto[] | undefined): Record<string, string> {
  return Object.fromEntries((vars ?? []).map((v) => [v.name, v.value]));
}
