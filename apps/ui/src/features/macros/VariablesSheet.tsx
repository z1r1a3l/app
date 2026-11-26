/**
 * Fork-only: per-chat variable inspector. Shows and edits the chat's
 * SillyTavern variables (`{{setvar}}` / `{{getvar}}`) and the global ones
 * (`{{setglobalvar}}`), e.g. to switch a preset's theme.
 */
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2, Variable } from "lucide-react";
import { Button, confirm, EmptyState, Input, Segmented, Sheet, Skeleton, Text, Textarea, icon, toast } from "@lettuceai/crisp";
import type { MacroVarDto } from "@/api/generated/bindings";
import { useMacroVars, useMacroVarsWrite } from "./data";

type Tab = "chat" | "global";

interface Row {
  key: number;
  name: string;
  value: string;
}

let nextKey = 1;
const rows = (vars: readonly MacroVarDto[]): Row[] => vars.map((v) => ({ key: nextKey++, name: v.name, value: v.value }));
const plain = (list: readonly Row[]) => JSON.stringify(list.map(({ name, value }) => [name, value]));

export function VariablesSheet({ open, conversationId, onClose }: { open: boolean; conversationId: string; onClose: () => void }) {
  const { t } = useTranslation("macros");
  const [tab, setTab] = useState<Tab>("chat");
  const query = useMacroVars(conversationId, open);
  const write = useMacroVarsWrite(conversationId);
  const saved = useMemo(() => {
    const data = query.data;
    return data ? rows(tab === "chat" ? data.chat : data.global) : null;
  }, [query.data, tab]);
  // The draft belongs to one saved snapshot; a new snapshot (tab switch,
  // save, refetch) resets it without an effect.
  const [edit, setEdit] = useState<{ base: Row[] | null; rows: Row[] | null }>({ base: null, rows: null });
  const draft = edit.base === saved ? edit.rows : saved;
  const setDraft = (next: (list: Row[] | null) => Row[] | null) =>
    setEdit((prev) => ({ base: saved, rows: next(prev.base === saved ? prev.rows : saved) }));
  const [filter, setFilter] = useState("");
  const dirty = draft !== null && saved !== null && plain(draft) !== plain(saved);
  const names = draft?.map((row) => row.name.trim()) ?? [];
  const duplicate = names.find((name, index) => name && names.indexOf(name) !== index);
  const missingName = draft?.some((row) => !row.name.trim()) ?? false;

  const leave = async (then: () => void) => {
    if (dirty) {
      const discard = await confirm({
        title: t("discard"),
        message: t("discardBody"),
        confirmLabel: t("discardConfirm"),
        cancelLabel: t("keepEditing"),
        destructive: true,
      });
      if (!discard) return;
    }
    then();
  };
  const save = () => {
    if (!draft) return;
    write.mutate(
      { global: tab === "global", vars: draft.map(({ name, value }) => ({ name: name.trim(), value })) },
      {
        onSuccess: () => toast.success(t("saved")),
        onError: () => toast.error(t("saveFailed")),
      },
    );
  };
  const update = (key: number, patch: Partial<Row>) => setDraft((list) => list?.map((row) => (row.key === key ? { ...row, ...patch } : row)) ?? null);
  const shown = (draft ?? []).filter((row) => !filter || row.name.toLowerCase().includes(filter.toLowerCase()) || row.value.toLowerCase().includes(filter.toLowerCase()));

  return (
    <Sheet
      open={open}
      onClose={() => void leave(onClose)}
      placement="auto"
      size="lg"
      title={t("title")}
      description={t("hint")}
      footer={
        <div className="flex w-full items-center gap-2">
          <Button variant="ghost" leading={<Plus size={icon.md} />} disabled={!draft} onClick={() => setDraft((list) => [...(list ?? []), { key: nextKey++, name: "", value: "" }])}>
            {t("add")}
          </Button>
          <div className="flex-1" />
          <Button variant="primary" disabled={!dirty || !!duplicate || missingName} loading={write.isPending} onClick={save}>
            {t("save")}
          </Button>
        </div>
      }
    >
      <div className="space-y-4 pb-4" data-testid="macro-vars">
        <Segmented
          block
          value={tab}
          onChange={(value) => void leave(() => setTab(value))}
          options={[
            { value: "chat", label: t("chat") },
            { value: "global", label: t("global") },
          ]}
        />
        <Text variant="hint" className="px-1">{tab === "chat" ? t("chatHint") : t("globalHint")}</Text>
        {(draft?.length ?? 0) > 6 && <Input value={filter} placeholder={t("filter")} onChange={(event) => setFilter(event.target.value)} />}
        {query.isPending && <Skeleton className="h-24 w-full" />}
        {query.isError && <Text variant="hint">{t("loadFailed")}</Text>}
        {draft && draft.length === 0 && <EmptyState size="sm" icon={<Variable size={icon.lg} />} title={t("empty")} description={tab === "chat" ? t("emptyChatHint") : t("emptyGlobalHint")} />}
        {duplicate && <Text variant="hint" className="text-danger">{t("duplicate", { name: duplicate })}</Text>}
        <div className="space-y-3">
          {shown.map((row) => (
            <div key={row.key} className="space-y-2 rounded-xl border border-line bg-surface-1 p-3">
              <div className="flex items-center gap-2">
                <Input
                  aria-label={t("name")}
                  className="font-mono text-sm"
                  value={row.name}
                  placeholder={t("name")}
                  spellCheck={false}
                  onChange={(event) => update(row.key, { name: event.target.value })}
                />
                <Button variant="ghost" aria-label={t("delete")} onClick={() => setDraft((list) => list?.filter((item) => item.key !== row.key) ?? null)}>
                  <Trash2 size={icon.md} />
                </Button>
              </div>
              <Textarea
                aria-label={t("value")}
                autoGrow
                maxRows={8}
                className="font-mono text-sm"
                spellCheck={false}
                value={row.value}
                placeholder={t("emptyValue")}
                onChange={(event) => update(row.key, { value: event.target.value })}
              />
            </div>
          ))}
        </div>
      </div>
    </Sheet>
  );
}
