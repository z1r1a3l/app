import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { RegexScopeRef, RegexScopeView, RegexScriptDto } from "@/api/generated/bindings";
import { useApi } from "@/api/use-api";
import { regexQueryKeys } from "./queries";

export type Scope = RegexScopeRef;

export const globalScope: Scope = { kind: "global", id: null };

export function useRegexScope(scope: Scope) {
  const api = useApi();
  return useQuery({
    queryKey: regexQueryKeys.scope(scope.kind, scope.id),
    queryFn: () => api.call("regexScopeGet", { scope }),
  });
}

export function useRegexWrite(scope: Scope) {
  const api = useApi();
  const client = useQueryClient();
  const done = (view: RegexScopeView) => {
    client.setQueryData(regexQueryKeys.scope(scope.kind, scope.id), view);
    void client.invalidateQueries({ queryKey: ["fork-regex", "conversation"] });
    void client.invalidateQueries({ queryKey: regexQueryKeys.pending() });
  };
  const replace = useMutation({
    mutationFn: (scripts: RegexScriptDto[]) => api.call("regexScopeReplace", { scope, scripts }),
    onSuccess: done,
  });
  const state = useMutation({
    mutationFn: (patch: { enabled?: boolean; pending?: boolean }) =>
      api.call("regexScopeStateSet", { scope, enabled: patch.enabled ?? null, pending: patch.pending ?? null }),
    onSuccess: done,
  });
  return { replace, state };
}

export function newScriptId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

/** SillyTavern's JSON shape, for export and for pasting into ST or Tavo. */
export function toSillyTavern(script: RegexScriptDto) {
  return {
    id: script.id,
    scriptName: script.script_name,
    findRegex: script.find_regex,
    replaceString: script.replace_string,
    trimStrings: script.trim_strings,
    placement: script.placement,
    disabled: script.disabled,
    markdownOnly: script.markdown_only,
    promptOnly: script.prompt_only,
    runOnEdit: script.run_on_edit,
    substituteRegex: script.substitute_regex,
    minDepth: script.min_depth,
    maxDepth: script.max_depth,
  };
}
