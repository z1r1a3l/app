import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Trash2 } from "lucide-react";
import {
  Button, Checkbox, Collapsible, confirm, Field, Input, NumberInput, Segmented, Select, Sheet,
  Switch, TagInput, Text, Textarea, icon,
} from "@lettuceai/crisp";
import type { RegexScriptDto } from "@/api/generated/bindings";
import { affectsOf, applyRegexScripts, compileFindRegex, placement, type Affects } from "./engine";
import { hasRichMarkup } from "./rich-html";
import { RichMessageBody } from "./RichMessageBody";

function withAffects(script: RegexScriptDto, affects: Affects): RegexScriptDto {
  return {
    ...script,
    markdown_only: affects === "display",
    prompt_only: affects === "prompt",
  };
}

const PLACEMENTS = [
  { value: placement.aiOutput, key: "ai" },
  { value: placement.userInput, key: "user" },
  { value: placement.reasoning, key: "reasoning" },
  { value: placement.worldInfo, key: "worldInfo" },
] as const;

export function RegexScriptSheet({
  script,
  isNew,
  onClose,
  onSave,
  onDelete,
}: {
  script: RegexScriptDto | null;
  isNew: boolean;
  onClose: () => void;
  onSave: (script: RegexScriptDto) => void;
  onDelete: (script: RegexScriptDto) => void;
}) {
  const { t } = useTranslation("regex");
  const [draft, setDraft] = useState<RegexScriptDto | null>(script);
  const [sample, setSample] = useState("");
  const [view, setView] = useState<"rendered" | "raw">("rendered");
  const valid = !draft?.find_regex || compileFindRegex(draft.find_regex) !== null;
  const affects = draft ? affectsOf(draft) : "display";
  const result = useMemo(() => {
    if (!draft || !sample) return null;
    const first = draft.placement[0] ?? placement.aiOutput;
    return applyRegexScripts([{ ...draft, disabled: false, min_depth: null, max_depth: null }], sample, {
      placement: first,
      target: affects === "prompt" ? "prompt" : "display",
    }, { user: "User", char: "Character" });
  }, [draft, sample, affects]);
  if (!draft) return <Sheet open={false} onClose={onClose}>{null}</Sheet>;
  const set = (patch: Partial<RegexScriptDto>) => setDraft({ ...draft, ...patch });
  const close = async () => {
    if (JSON.stringify(draft) !== JSON.stringify(script)) {
      const discard = await confirm({
        title: t("editor.discard"),
        message: t("editor.discardBody"),
        confirmLabel: t("editor.discardConfirm"),
        cancelLabel: t("editor.keepEditing"),
        destructive: true,
      });
      if (!discard) return;
    }
    onClose();
  };
  const togglePlacement = (value: number, on: boolean) =>
    set({
      placement: on
        ? [...new Set([...draft.placement, value])].sort()
        : draft.placement.filter((item) => item !== value),
    });
  return (
    <Sheet
      open
      onClose={() => void close()}
      placement="auto"
      size="lg"
      title={isNew ? t("editor.new") : t("editor.edit")}
      footer={
        <div className="flex w-full items-center gap-2">
          {!isNew && (
            <Button
              variant="ghost"
              leading={<Trash2 size={icon.md} />}
              onClick={async () => {
                if (await confirm({ title: t("editor.deleteConfirm"), destructive: true, confirmLabel: t("editor.delete") })) onDelete(draft);
              }}
            >
              {t("editor.delete")}
            </Button>
          )}
          <div className="flex-1" />
          <Button variant="primary" disabled={!valid || !draft.find_regex} onClick={() => onSave(draft)}>
            {t("editor.save")}
          </Button>
        </div>
      }
    >
      <div className="space-y-5 pb-4" data-testid="regex-editor">
        <Field label={t("editor.name")}>
          <Input
            value={draft.script_name}
            placeholder={t("editor.namePlaceholder")}
            onChange={(event) => set({ script_name: event.target.value })}
          />
        </Field>
        <Field label={t("editor.find")} hint={t("editor.findHint")} error={valid ? undefined : t("invalid")}>
          <Textarea
            autoGrow
            maxRows={8}
            className="font-mono text-sm"
            spellCheck={false}
            value={draft.find_regex}
            onChange={(event) => set({ find_regex: event.target.value })}
          />
        </Field>
        <Field label={t("editor.replace")} hint={t("editor.replaceHint")}>
          <Textarea
            autoGrow
            maxRows={10}
            className="font-mono text-sm"
            spellCheck={false}
            value={draft.replace_string}
            onChange={(event) => set({ replace_string: event.target.value })}
          />
        </Field>
        <Field label={t("editor.affects")} hint={t(`editor.${affects}Hint`)}>
          <Segmented
            block
            value={affects}
            onChange={(value) => setDraft(withAffects(draft, value))}
            options={[
              { value: "display", label: t("editor.display") },
              { value: "prompt", label: t("editor.prompt") },
              { value: "both", label: t("editor.both") },
            ]}
            aria-label={t("editor.affects")}
          />
        </Field>
        <Field label={t("editor.applies")}>
          <div className="flex flex-wrap gap-x-5 gap-y-2">
            {PLACEMENTS.map((item) => (
              <Checkbox
                key={item.value}
                checked={draft.placement.includes(item.value)}
                onChange={(on) => togglePlacement(item.value, on)}
                label={t(`editor.${item.key}`)}
              />
            ))}
          </div>
        </Field>
        <Field label={t("editor.enabled")} inline>
          <Switch checked={!draft.disabled} onChange={(on) => set({ disabled: !on })} aria-label={t("editor.enabled")} />
        </Field>
        <Collapsible title={t("editor.tester")} description={t("editor.testerHint")} defaultOpen variant="panel">
          <div className="space-y-3">
            <Textarea
              autoGrow
              maxRows={10}
              aria-label={t("editor.testerInput")}
              placeholder={t("editor.testerInput")}
              value={sample}
              onChange={(event) => setSample(event.target.value)}
            />
            {result !== null && (
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Text variant="label">{t("editor.result")}</Text>
                  <Segmented
                    size="sm"
                    value={view}
                    onChange={setView}
                    options={[
                      { value: "rendered", label: t("editor.rendered") },
                      { value: "raw", label: t("editor.raw") },
                    ]}
                  />
                </div>
                {result === sample && <Text variant="hint">{t("editor.noMatch")}</Text>}
                <div className="rounded-lg border border-line bg-surface-2 p-3 text-sm" data-testid="regex-result">
                  {view === "raw" || !hasRichMarkup(result) ? (
                    <pre className="whitespace-pre-wrap font-mono text-xs">{result}</pre>
                  ) : (
                    <RichMessageBody text={result} />
                  )}
                </div>
              </div>
            )}
          </div>
        </Collapsible>
        <Collapsible title={t("editor.advanced")} variant="panel">
          <div className="space-y-4">
            <Field label={t("editor.minDepth")} hint={t("editor.minDepthHint")}>
              <NumberInput
                value={draft.min_depth ?? 0}
                min={0}
                max={1000}
                onChange={(value) => set({ min_depth: value > 0 ? value : null })}
              />
            </Field>
            <Field label={t("editor.maxDepth")} hint={t("editor.maxDepthHint")}>
              <div className="flex items-center gap-3">
                <Switch
                  checked={draft.max_depth !== null}
                  onChange={(on) => set({ max_depth: on ? 5 : null })}
                  aria-label={t("editor.limit")}
                />
                {draft.max_depth !== null && (
                  <NumberInput value={draft.max_depth} min={0} max={1000} onChange={(value) => set({ max_depth: value })} />
                )}
              </div>
            </Field>
            <Field label={t("editor.trim")} hint={t("editor.trimHint")}>
              <TagInput value={draft.trim_strings} onChange={(value) => set({ trim_strings: value })} />
            </Field>
            <Field label={t("editor.macros")}>
              <Select
                value={String(draft.substitute_regex)}
                onChange={(event) => set({ substitute_regex: Number(event.target.value) })}
                options={[
                  { value: "0", label: t("editor.macrosNone") },
                  { value: "1", label: t("editor.macrosRaw") },
                  { value: "2", label: t("editor.macrosEscaped") },
                ]}
              />
            </Field>
          </div>
        </Collapsible>
      </div>
    </Sheet>
  );
}
