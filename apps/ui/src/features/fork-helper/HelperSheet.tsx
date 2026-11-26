/**
 * Fork-only: the helper model's panel in chat tools: switches for the chat
 * and its character, trackers, the helper's model and prompt, and editors
 * for the tracked state and the author's notebook.
 */
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { BookOpen, Cpu, NotebookPen } from "lucide-react";
import {
  Button,
  confirm,
  EmptyState,
  Field,
  ListGroup,
  ListRow,
  ListRowButton,
  NumberInput,
  Segmented,
  Sheet,
  Skeleton,
  Switch,
  Text,
  Textarea,
  icon,
  toast,
} from "@lettuceai/crisp";
import type { HelperChatView, HelperStateDto, HelperTrackersDto } from "@/api/generated/bindings";
import { queryKeys } from "@/api/query-keys";
import { useApi } from "@/api/use-api";
import { meetsRequirement, ModelChoiceSheet, modelChoices, type ModelRequirement } from "@/entities/model";
import { useHelperChat, useHelperConfig, useHelperConfigWrite, useHelperScopeWrite, useHelperStateWrite } from "./data";
import { EntitiesEditor, FieldsEditor, InventoryEditor, NotebookEditor } from "./editors";

type Tab = "settings" | "state" | "notebook";

const LOCAL = new Set(["ollama", "llama_cpp", "stable_diffusion"]);
/** Cloud chat models only: the helper runs next to the main model. */
export const helperModelRequirement: ModelRequirement = {
  kind: "chat",
  input: ["text"],
  output: ["text"],
  allow: (choice) => !LOCAL.has(choice.account.protocol),
};

const TRACKERS = ["scene", "characters", "bonds", "inventory", "npcs", "notebook"] as const;

/** Where tracker switches are written: the row in effect, else the chat. */
export function trackerTarget(view: HelperChatView): { scope: "conversation" | "character"; id: string; enabled: boolean } {
  if (view.chat) return { scope: "conversation", id: view.conversation_id, enabled: view.chat.enabled };
  if (view.character?.enabled && view.character_id) return { scope: "character", id: view.character_id, enabled: true };
  return { scope: "conversation", id: view.conversation_id, enabled: false };
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export function HelperSheet({ open, conversationId, onClose }: { open: boolean; conversationId: string; onClose: () => void }) {
  const { t } = useTranslation("helper");
  const [tab, setTab] = useState<Tab>("settings");
  const chat = useHelperChat(conversationId, open);
  const view = chat.data;
  const saveState = useHelperStateWrite(conversationId);

  // One draft of the whole state for the State and Notebook tabs; a new
  // snapshot from the app resets it.
  const saved = view?.state ?? null;
  const [edit, setEdit] = useState<{ base: HelperStateDto | null; draft: HelperStateDto | null }>({ base: null, draft: null });
  const draft = edit.base === saved ? edit.draft ?? saved : saved;
  const patch = (next: Partial<HelperStateDto>) => {
    if (!draft) return;
    setEdit({ base: saved, draft: { ...draft, ...next } });
  };
  const dirty = draft !== null && saved !== null && !same(draft, saved);

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
      setEdit({ base: null, draft: null });
    }
    then();
  };
  const save = () => {
    if (!draft) return;
    saveState.mutate(draft, {
      onSuccess: () => toast.success(t("saved")),
      onError: () => toast.error(t("saveFailed")),
    });
  };

  return (
    <Sheet
      open={open}
      onClose={() => void leave(onClose)}
      placement="auto"
      size="lg"
      title={t("title")}
      description={t("hint")}
      {...(tab !== "settings"
        ? {
            footer: (
              <div className="flex w-full justify-end">
                <Button variant="primary" disabled={!dirty} loading={saveState.isPending} onClick={save}>
                  {t("save")}
                </Button>
              </div>
            ),
          }
        : {})}
    >
      <div className="space-y-4 pb-4" data-testid="helper-sheet">
        <Segmented
          block
          value={tab}
          onChange={setTab}
          options={[
            { value: "settings", label: t("tabs.settings") },
            { value: "state", label: t("tabs.state") },
            { value: "notebook", label: t("tabs.notebook") },
          ]}
        />
        {chat.isPending && <Skeleton className="h-24 w-full" />}
        {chat.isError && <Text variant="hint">{t("loadFailed")}</Text>}
        {view && tab === "settings" && <SettingsTab view={view} open={open} />}
        {view && draft && tab === "state" && (
          <div className="space-y-5">
            {!view.enabled && <Text variant="hint">{t("off")}</Text>}
            <Text variant="hint" className="px-1">{t("stateHint")}</Text>
            {same(draft, { ...draft, scene: [], characters: [], bonds: [], inventory: [], npcs: [] }) && (
              <EmptyState size="sm" icon={<BookOpen size={icon.lg} />} title={t("stateEmpty")} description={t("stateEmptyHint")} />
            )}
            <section className="space-y-2">
              <Text variant="label" className="px-1">{t("scene")}</Text>
              <FieldsEditor fields={draft.scene} onChange={(scene) => patch({ scene })} />
            </section>
            <EntitiesEditor label={t("characters")} entities={draft.characters} onChange={(characters) => patch({ characters })} />
            <EntitiesEditor label={t("bonds")} entities={draft.bonds} onChange={(bonds) => patch({ bonds })} />
            <InventoryEditor holders={draft.inventory} onChange={(inventory) => patch({ inventory })} />
            <EntitiesEditor label={t("npcs")} entities={draft.npcs} onChange={(npcs) => patch({ npcs })} />
          </div>
        )}
        {view && draft && tab === "notebook" && (
          <div className="space-y-3">
            {!view.enabled && <Text variant="hint">{t("off")}</Text>}
            {draft.notebook.length === 0 && (
              <EmptyState size="sm" icon={<NotebookPen size={icon.lg} />} title={t("notebookEmpty")} description={t("notebookEmptyHint")} />
            )}
            <NotebookEditor truths={draft.notebook} onChange={(notebook) => patch({ notebook })} />
          </div>
        )}
      </div>
    </Sheet>
  );
}

function SettingsTab({ view, open }: { view: HelperChatView; open: boolean }) {
  const { t } = useTranslation("helper");
  const api = useApi();
  const scope = useHelperScopeWrite(view.conversation_id);
  const config = useHelperConfig(open);
  const writeConfig = useHelperConfigWrite();
  const [picker, setPicker] = useState(false);
  const models = useQuery({ queryKey: queryKeys.models.list(), queryFn: () => api.call("modelsList"), enabled: open });
  const accounts = useQuery({ queryKey: queryKeys.providers.accounts(), queryFn: () => api.call("providerAccountsList"), enabled: open });
  const choices = useMemo(
    () => modelChoices(models.data?.models ?? [], accounts.data ?? []).filter((choice) => meetsRequirement(choice, helperModelRequirement)),
    [models.data, accounts.data],
  );
  const current = config.data;
  const model = choices.find((choice) => choice.model.id === current?.model_profile_id);
  const [promptEdit, setPromptEdit] = useState<{ base: string; text: string } | null>(null);
  const savedPrompt = current ? current.prompt ?? current.default_prompt : "";
  const prompt = promptEdit && promptEdit.base === savedPrompt ? promptEdit.text : savedPrompt;
  const promptDirty = prompt !== savedPrompt;

  const failed = () => toast.error(t("saveFailed"));
  const setScope = (target: { scope: "conversation" | "character"; id: string }, enabled: boolean, trackers: HelperTrackersDto) =>
    scope.mutate({ scope: target.scope, id: target.id, enabled, trackers }, { onError: failed });
  const writeConfigPatch = (next: { model_profile_id?: string | null; prompt?: string | null; history?: number }) => {
    if (!current) return;
    writeConfig.mutate(
      {
        model_profile_id: next.model_profile_id !== undefined ? next.model_profile_id : current.model_profile_id,
        prompt: next.prompt !== undefined ? next.prompt : current.prompt,
        history: next.history ?? current.history,
      },
      { onError: failed, onSuccess: () => setPromptEdit(null) },
    );
  };
  const target = trackerTarget(view);

  return (
    <div className="space-y-5">
      <ListGroup>
        <ListRow
          title={t("chatSwitch")}
          description={view.chat ? t("chatSwitchHint") : t("chatFollows")}
          trailing={
            <Switch
              checked={view.chat ? view.chat.enabled : view.enabled}
              disabled={scope.isPending}
              aria-label={t("chatSwitch")}
              onChange={(enabled) => setScope({ scope: "conversation", id: view.conversation_id }, enabled, view.trackers)}
            />
          }
        />
        {view.character_id ? (
          <ListRow
            title={t("characterSwitch", { name: view.character_name ?? "" })}
            description={t("characterSwitchHint")}
            trailing={
              <Switch
                checked={view.character?.enabled ?? false}
                disabled={scope.isPending}
                aria-label={t("characterSwitch", { name: view.character_name ?? "" })}
                onChange={(enabled) =>
                  setScope({ scope: "character", id: view.character_id ?? "" }, enabled, view.character?.trackers ?? view.trackers)
                }
              />
            }
          />
        ) : null}
      </ListGroup>
      {!view.character_id && <Text variant="hint" className="px-1">{t("groupHint")}</Text>}

      <ListGroup label={t("trackers")}>
        {TRACKERS.map((key) => (
          <ListRow
            key={key}
            title={t(`tracker.${key}`)}
            description={t(`trackerHint.${key}`)}
            trailing={
              <Switch
                checked={view.trackers[key]}
                disabled={scope.isPending}
                aria-label={t(`tracker.${key}`)}
                onChange={(on) => setScope(target, target.enabled, { ...view.trackers, [key]: on })}
              />
            }
          />
        ))}
      </ListGroup>

      <ListGroup>
        <ListRowButton
          icon={<Cpu size={icon.lg} />}
          title={t("model")}
          description={model ? model.model.display_name : t("modelDefault")}
          onClick={() => setPicker(true)}
        />
        <ListRow
          title={t("history")}
          trailing={
            current ? (
              <Field label={<span className="sr-only">{t("history")}</span>} className="space-y-0">
                <NumberInput className="w-28" value={current.history} min={0} max={40} step={1} precision={0} onChange={(history) => writeConfigPatch({ history })} />
              </Field>
            ) : (
              <Skeleton className="h-8 w-20" />
            )
          }
        />
      </ListGroup>

      <section className="space-y-2">
        <Text variant="label" className="px-1">{t("prompt")}</Text>
        <Text variant="hint" className="px-1">{t("promptHint")}</Text>
        <Textarea
          aria-label={t("prompt")}
          autoGrow
          maxRows={16}
          className="font-mono text-xs"
          spellCheck={false}
          value={prompt}
          onChange={(e) => setPromptEdit({ base: savedPrompt, text: e.target.value })}
        />
        <div className="flex gap-2">
          <Button variant="ghost" disabled={!current?.prompt && !promptDirty} onClick={() => writeConfigPatch({ prompt: null })}>
            {t("promptReset")}
          </Button>
          <div className="flex-1" />
          <Button variant="primary" disabled={!promptDirty} loading={writeConfig.isPending} onClick={() => writeConfigPatch({ prompt })}>
            {t("save")}
          </Button>
        </div>
      </section>

      <ModelChoiceSheet
        api={api}
        open={picker}
        onClose={() => setPicker(false)}
        title={t("model")}
        value={current?.model_profile_id ?? null}
        requirement={helperModelRequirement}
        nullChoice={{ title: t("modelDefault"), description: t("modelHint") }}
        onChange={(id) => {
          writeConfigPatch({ model_profile_id: id });
          setPicker(false);
        }}
      />
    </div>
  );
}
