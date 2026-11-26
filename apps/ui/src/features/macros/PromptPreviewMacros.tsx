/**
 * Fork-only: the prompt preview after SillyTavern macros. Entries are
 * evaluated in the order the model reads them (before the chat, then the
 * in-chat ones deepest first), nothing is saved, and the variables they
 * leave behind are listed (see usePromptMacros for the evaluation).
 */
import { useTranslation } from "react-i18next";
import { Collapsible } from "@lettuceai/crisp";
import type { MacroPreviewResult, MacroVarDto } from "@/api/generated/bindings";
import { rollLine } from "./rolls";

function VarList({ vars, changed }: { vars: readonly MacroVarDto[]; changed: ReadonlySet<string> }) {
  return (
    <dl className="divide-y divide-line">
      {vars.map((v) => (
        <div key={v.name} className="flex items-start justify-between gap-3 px-4 py-2 text-sm">
          <dt className={`min-w-0 shrink-0 font-mono ${changed.has(v.name) ? "text-accent" : "text-fg-2"}`}>{v.name}</dt>
          <dd className="min-w-0 whitespace-pre-line text-right font-mono wrap-anywhere text-fg-3">{v.value || "∅"}</dd>
        </div>
      ))}
    </dl>
  );
}

export function PromptPreviewMacrosPanel({ result, defaultOpen }: { result: MacroPreviewResult | null; defaultOpen: boolean }) {
  const { t } = useTranslation("macros");
  if (!result) return null;
  const changed = new Set(result.changed);
  const { chat, global } = result.vars;
  return (
    <Collapsible defaultOpen={defaultOpen} title={t("preview.title")} description={t("preview.hint")}>
      <div className="space-y-3 pb-2" data-testid="macro-preview-vars">
        {chat.length > 0 && (
          <div>
            <p className="px-4 pt-2 text-2xs font-medium uppercase tracking-wide text-fg-3">{t("preview.chat", { count: chat.length })}</p>
            <VarList vars={chat} changed={changed} />
          </div>
        )}
        {global.length > 0 && (
          <div>
            <p className="px-4 pt-2 text-2xs font-medium uppercase tracking-wide text-fg-3">{t("preview.global", { count: global.length })}</p>
            <VarList vars={global} changed={changed} />
          </div>
        )}
        {chat.length === 0 && global.length === 0 && <p className="px-4 py-2 text-sm text-fg-3">{t("preview.none")}</p>}
        {result.rolls.length > 0 && (
          <div>
            <p className="px-4 pt-2 text-2xs font-medium uppercase tracking-wide text-fg-3">{t("preview.rolls")}</p>
            <ul className="px-4 text-sm font-mono text-fg-2">
              {result.rolls.map((roll, index) => <li key={index}>{rollLine(roll)}</li>)}
            </ul>
          </div>
        )}
        {changed.size > 0 && <p className="px-4 text-xs text-fg-3">{t("preview.changedHint")}</p>}
      </div>
    </Collapsible>
  );
}
