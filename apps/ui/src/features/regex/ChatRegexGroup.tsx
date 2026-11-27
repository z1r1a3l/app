// Fork: "Regex and macros of this chat" in chat settings. Switches for the
// global and the characters' scripts in this chat, the chat's own scripts
// (the `chat` regex scope) and its own `{{key}}` macros.
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Regex, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button, icon, IconButton, Input, ListGroup, ListRow, ListRowButton, Sheet, Switch, Textarea, toast } from "@lettuceai/crisp";
import type { ChatMacroDto, ChatRegexSettingsDto } from "@/api/generated/bindings";
import { useApi } from "@/api/use-api";
import { regexQueryKeys } from "./queries";
import { RegexScopeList } from "./RegexPage";

const settingsKey = (conversationId: string) => ["fork-regex", "chat-settings", conversationId] as const;

export function ChatRegexGroup({ conversationId }: { conversationId: string }) {
  const { t } = useTranslation("fork-ui");
  const [open, setOpen] = useState(false);
  const api = useApi();
  const settings = useQuery({ queryKey: settingsKey(conversationId), queryFn: () => api.call("chatRegexSettingsGet", { conversation_id: conversationId }) });
  const data = settings.data;
  const summary = data
    ? [data.skip_global && t("chatRegex.globalOff"), data.skip_character && t("chatRegex.characterOff"), data.macros.length > 0 && t("chatRegex.macroCount", { count: data.macros.length })].filter(Boolean).join(" · ")
    : "";
  return (
    <ListGroup label={t("chatRegex.section")}>
      <ListRowButton icon={<Regex size={icon.lg} />} title={t("chatRegex.title")} description={summary || t("chatRegex.hint", { example: "{{key}}" })} onClick={() => setOpen(true)} />
      <Sheet open={open} onClose={() => setOpen(false)} size="lg" title={t("chatRegex.title")}>
        {open && data && <ChatRegexBody key={JSON.stringify(data.macros)} conversationId={conversationId} settings={data} />}
      </Sheet>
    </ListGroup>
  );
}

function ChatRegexBody({ conversationId, settings }: { conversationId: string; settings: ChatRegexSettingsDto }) {
  const { t } = useTranslation("fork-ui");
  const api = useApi();
  const client = useQueryClient();
  const write = useMutation({
    mutationFn: (next: ChatRegexSettingsDto) => api.call("chatRegexSettingsSet", next),
    onSuccess: (view) => {
      client.setQueryData(settingsKey(conversationId), view);
      void client.invalidateQueries({ queryKey: regexQueryKeys.conversation(conversationId) });
    },
  });
  const [macros, setMacros] = useState<ChatMacroDto[]>(settings.macros);
  const dirty = JSON.stringify(macros) !== JSON.stringify(settings.macros);
  const save = (patch: Partial<ChatRegexSettingsDto>) => write.mutateAsync({ ...settings, ...patch });
  return (
    <div className="space-y-5 pb-2">
      <ListGroup label={t("chatRegex.inherited")}>
        <ListRow
          title={t("chatRegex.useGlobal")}
          description={t("chatRegex.useGlobalHint")}
          trailing={<Switch checked={!settings.skip_global} aria-label={t("chatRegex.useGlobal")} onChange={(on) => void save({ skip_global: !on })} />}
        />
        <ListRow
          title={t("chatRegex.useCharacter")}
          description={t("chatRegex.useCharacterHint")}
          trailing={<Switch checked={!settings.skip_character} aria-label={t("chatRegex.useCharacter")} onChange={(on) => void save({ skip_character: !on })} />}
        />
      </ListGroup>
      <RegexScopeList scope={{ kind: "chat", id: conversationId }} title={t("chatRegex.own")} hint={t("chatRegex.ownHint")} switchLabel={t("chatRegex.ownSwitch")} />
      <section className="space-y-2">
        <h3 className="px-1 text-2xs font-medium uppercase tracking-[0.18em] text-fg-3">{t("chatRegex.macros")}</h3>
        <p className="px-1 text-sm text-fg-3">{t("chatRegex.macrosHint", { example: "{{key}}" })}</p>
        {macros.map((item, index) => (
          <div key={index} className="flex items-start gap-2 rounded-xl border border-line bg-surface-1 p-2">
            <div className="min-w-0 flex-1 space-y-2">
              <Input value={item.key} placeholder={t("chatRegex.key")} aria-label={t("chatRegex.key")} onChange={(event) => setMacros(macros.map((m, i) => (i === index ? { ...m, key: event.target.value } : m)))} />
              <Textarea value={item.value} rows={2} placeholder={t("chatRegex.value")} aria-label={t("chatRegex.value")} onChange={(event) => setMacros(macros.map((m, i) => (i === index ? { ...m, value: event.target.value } : m)))} />
            </div>
            <IconButton label={t("chatRegex.remove")} onClick={() => setMacros(macros.filter((_, i) => i !== index))}>
              <Trash2 size={icon.md} />
            </IconButton>
          </div>
        ))}
        <div className="flex flex-wrap gap-2">
          <Button variant="tonal" size="sm" leading={<Plus size={icon.sm} />} onClick={() => setMacros([...macros, { key: "", value: "" }])}>{t("chatRegex.addMacro")}</Button>
          <Button
            variant="primary"
            size="sm"
            disabled={!dirty || write.isPending}
            onClick={() =>
              void save({ macros }).then((view) => {
                setMacros(view.macros);
                toast.success(t("chatRegex.saved"));
              })
            }
          >
            {t("chatRegex.save")}
          </Button>
        </div>
      </section>
    </div>
  );
}
