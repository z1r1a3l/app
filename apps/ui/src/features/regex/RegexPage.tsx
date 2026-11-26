/**
 * Fork-only: Settings → Regex scripts. Global scripts, a prompt preset's
 * scripts and a character's scripts, each a simple ordered list.
 */
import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ArrowDown, ArrowUp, Copy, FileText, Plus, Regex, Trash2, Upload, User } from "lucide-react";
import {
  Alert, Badge, Button, confirm, EmptyState, IconButton, ListGroup, ListRow, ListRowButton, Page,
  Skeleton, Switch, Text, icon, toast,
} from "@lettuceai/crisp";
import type { RegexScriptDto } from "@/api/generated/bindings";
import { useApi } from "@/api/use-api";
import { promptsQuery } from "@/entities/prompt";
import { globalScope, newScriptId, toSillyTavern, useRegexScope, useRegexWrite, type Scope } from "./data";
import { affectsOf, compileFindRegex, newScript } from "./engine";
import { regexQueryKeys } from "./queries";
import { RegexScriptSheet } from "./RegexScriptSheet";

function useCharacterName(id: string | null) {
  const api = useApi();
  return useQuery({
    queryKey: ["fork-regex", "character-name", id],
    queryFn: async () => {
      const view = await api.call("characterGet", { character_id: id! });
      return (view as { name?: string; profile?: { name?: string } }).name ?? (view as { profile?: { name?: string } }).profile?.name ?? "";
    },
    enabled: id !== null,
    staleTime: 60_000,
  }).data;
}

function ScriptRow({
  script,
  index,
  total,
  onOpen,
  onToggle,
  onMove,
}: {
  script: RegexScriptDto;
  index: number;
  total: number;
  onOpen: () => void;
  onToggle: (on: boolean) => void;
  onMove: (delta: -1 | 1) => void;
}) {
  const { t } = useTranslation("regex");
  const invalid = compileFindRegex(script.find_regex) === null;
  const where = script.placement
    .map((value) => (value === 2 ? t("summary.ai") : value === 1 ? t("summary.user") : t("summary.other")))
    .filter((value, i, all) => all.indexOf(value) === i)
    .join(", ");
  return (
    <ListRowButton
      icon={<Regex size={icon.lg} />}
      title={script.script_name || t("untitled")}
      description={
        <span className="flex flex-wrap items-center gap-1.5">
          <Badge tone={affectsOf(script) === "prompt" ? "info" : affectsOf(script) === "both" ? "warning" : "neutral"}>
            {t(`summary.${affectsOf(script)}`)}
          </Badge>
          <span>{where}</span>
          {invalid && <Badge tone="danger">{t("invalid")}</Badge>}
        </span>
      }
      chevron={false}
      onClick={onOpen}
      trailing={
        <span className="flex items-center gap-1" onClick={(event) => event.stopPropagation()}>
          <IconButton label={t("moveUp")} size="sm" variant="ghost" disabled={index === 0} onClick={() => onMove(-1)}>
            <ArrowUp size={icon.sm} />
          </IconButton>
          <IconButton label={t("moveDown")} size="sm" variant="ghost" disabled={index === total - 1} onClick={() => onMove(1)}>
            <ArrowDown size={icon.sm} />
          </IconButton>
          <Switch checked={!script.disabled} onChange={onToggle} aria-label={script.script_name} />
        </span>
      }
    />
  );
}

/** One scope's list: its switch, its scripts and add / import / copy. */
export function RegexScopeList({ scope, title, hint, switchLabel }: { scope: Scope; title: string; hint: string; switchLabel: string }) {
  const { t } = useTranslation("regex");
  const api = useApi();
  const view = useRegexScope(scope);
  const { replace, state } = useRegexWrite(scope);
  const [editing, setEditing] = useState<{ script: RegexScriptDto; isNew: boolean } | null>(null);
  const file = useRef<HTMLInputElement>(null);
  const scripts = view.data?.scripts ?? [];
  const save = (next: RegexScriptDto[]) => replace.mutateAsync(next);
  const importFile = async (picked: File | undefined) => {
    if (!picked) return;
    try {
      const parsed = await api.call("regexImportParse", { json: await picked.text() });
      const known = new Set(scripts.map((script) => script.id));
      const fresh = parsed.scripts.map((script) => (known.has(script.id) ? { ...script, id: newScriptId() } : script));
      await save([...scripts, ...fresh]);
      toast.success(t("imported", { count: fresh.length }));
    } catch {
      toast.error(t("importFailed"));
    }
  };
  return (
    <section className="space-y-2">
      <ListGroup label={title}>
        <ListRow
          title={switchLabel}
          description={hint}
          trailing={
            <Switch
              checked={view.data?.enabled ?? true}
              onChange={(on) => void state.mutateAsync({ enabled: on })}
              aria-label={switchLabel}
            />
          }
        />
        {view.isPending ? (
          <ListRow title={<Skeleton className="h-5 w-40" />} description={<Skeleton className="h-4 w-56" />} />
        ) : scripts.length === 0 ? null : (
          scripts.map((script, index) => (
            <ScriptRow
              key={script.id}
              script={script}
              index={index}
              total={scripts.length}
              onOpen={() => setEditing({ script, isNew: false })}
              onToggle={(on) => void save(scripts.map((item) => (item.id === script.id ? { ...item, disabled: !on } : item)))}
              onMove={(delta) => {
                const next = [...scripts];
                const [moved] = next.splice(index, 1);
                next.splice(index + delta, 0, moved!);
                void save(next);
              }}
            />
          ))
        )}
      </ListGroup>
      {!view.isPending && scripts.length === 0 && (
        <EmptyState size="sm" icon={<Regex size={icon.lg} />} title={t("noScripts")} description={t("noScriptsHint")} />
      )}
      <div className="flex flex-wrap gap-2">
        <Button variant="tonal" size="sm" leading={<Plus size={icon.sm} />} onClick={() => setEditing({ script: newScript(newScriptId()), isNew: true })}>
          {t("add")}
        </Button>
        <Button variant="secondary" size="sm" leading={<Upload size={icon.sm} />} onClick={() => file.current?.click()}>
          {t("import")}
        </Button>
        {scripts.length > 0 && (
          <>
            <Button
              variant="ghost"
              size="sm"
              leading={<Copy size={icon.sm} />}
              onClick={async () => {
                await navigator.clipboard.writeText(JSON.stringify(scripts.map(toSillyTavern), null, 2));
                toast.success(t("exported"));
              }}
            >
              {t("export")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              leading={<Trash2 size={icon.sm} />}
              onClick={async () => {
                if (await confirm({ title: t("deleteAllConfirm", { count: scripts.length }), destructive: true, confirmLabel: t("deleteAll") })) void save([]);
              }}
            >
              {t("deleteAll")}
            </Button>
          </>
        )}
        <input
          ref={file}
          type="file"
          accept=".json,application/json"
          className="hidden"
          onChange={(event) => {
            void importFile(event.target.files?.[0]);
            event.target.value = "";
          }}
        />
      </div>
      {editing && (
        <RegexScriptSheet
          key={editing.script.id}
          script={editing.script}
          isNew={editing.isNew}
          onClose={() => setEditing(null)}
          onSave={async (script) => {
            await save(editing.isNew ? [...scripts, script] : scripts.map((item) => (item.id === script.id ? script : item)));
            setEditing(null);
          }}
          onDelete={async (script) => {
            await save(scripts.filter((item) => item.id !== script.id));
            setEditing(null);
          }}
        />
      )}
    </section>
  );
}

/** An imported preset's or card's scripts, waiting for the user's decision. */
function PendingCard({ scope, onReview }: { scope: Scope; onReview: () => void }) {
  const { t } = useTranslation("regex");
  const api = useApi();
  const view = useRegexScope(scope);
  const { state } = useRegexWrite(scope);
  const preset = scope.kind === "preset";
  const prompts = useQuery({ ...promptsQuery(api, "direct_chat"), enabled: preset });
  const characterName = useCharacterName(preset ? null : scope.id) ?? "";
  const name = preset ? (prompts.data?.find((prompt) => prompt.id === scope.id)?.name ?? "") : characterName;
  if (!view.data?.pending) return null;
  return (
    <Alert
      tone="warning"
      title={t(preset ? "pending.presetTitle" : "pending.title")}
      description={t(preset ? "pending.presetBody" : "pending.body", { name, count: view.data.scripts.length })}
      action={
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="primary" onClick={() => void state.mutateAsync({ pending: false, enabled: true })}>
            {t("pending.allow")}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => void state.mutateAsync({ pending: false, enabled: false })}>
            {t("pending.keepOff")}
          </Button>
          <Button size="sm" variant="ghost" onClick={onReview}>
            {t("pending.review")}
          </Button>
        </div>
      }
    />
  );
}

function ScopeTitle({ scope }: { scope: Scope }) {
  const api = useApi();
  const prompts = useQuery({ ...promptsQuery(api, "direct_chat"), enabled: scope.kind === "preset" });
  const characterName = useCharacterName(scope.kind === "character" ? scope.id : null);
  if (scope.kind === "preset") return <>{prompts.data?.find((prompt) => prompt.id === scope.id)?.name ?? scope.id}</>;
  return <>{characterName || scope.id}</>;
}

export interface RegexPageLayout {
  phone: boolean;
  back: { onBack?: () => void; backLabel?: string };
}

export function RegexPage({ layout }: { layout: RegexPageLayout }) {
  const { t } = useTranslation("regex");
  const api = useApi();
  const client = useQueryClient();
  const { phone, back } = layout;
  const [open, setOpen] = useState<Scope | null>(null);
  const prompts = useQuery(promptsQuery(api, "direct_chat"));
  const scopes = useQuery({ queryKey: [...regexQueryKeys.all, "scopes"], queryFn: () => api.call("regexScopesList") });
  const pending = useQuery({ queryKey: regexQueryKeys.pending(), queryFn: () => api.call("regexPendingScopes") });
  const counts = new Map((scopes.data ?? []).map((item) => [`${item.scope.kind}:${item.scope.id}`, item.scripts]));
  const characterScopes = (scopes.data ?? []).filter((item) => item.scope.kind === "character");
  const refresh = () => void client.invalidateQueries({ queryKey: regexQueryKeys.all });
  if (open) {
    return (
      <Page
        title={<ScopeTitle scope={open} />}
        meta={t(open.kind === "preset" ? "presets" : "characters")}
        onBack={() => { setOpen(null); refresh(); }}
        backLabel={t("back")}
        contentClassName="max-w-3xl space-y-7"
      >
        <RegexScopeList
          scope={open}
          title={t(open.kind === "preset" ? "presets" : "characters")}
          hint={t(open.kind === "preset" ? "presetsHint" : "charactersHint")}
          switchLabel={t(open.kind === "preset" ? "presetSwitch" : "characterSwitch")}
        />
      </Page>
    );
  }
  return (
    <Page title={t("title")} meta={phone ? undefined : t("meta")} {...back} contentClassName="max-w-3xl space-y-7">
      <Text variant="secondary">{t("intro")}</Text>
      {(pending.data ?? []).map((scope) => (
        <PendingCard key={`${scope.kind}:${scope.id}`} scope={scope} onReview={() => setOpen(scope)} />
      ))}
      <RegexScopeList scope={globalScope} title={t("global")} hint={t("globalHint")} switchLabel={t("globalSwitch")} />
      <ListGroup label={t("presets")}>
        {(prompts.data ?? []).map((prompt) => {
          const count = counts.get(`preset:${prompt.id}`) ?? 0;
          return (
            <ListRowButton
              key={prompt.id}
              icon={<FileText size={icon.lg} />}
              title={prompt.name}
              description={count ? t("count", { count }) : t("noScripts")}
              onClick={() => setOpen({ kind: "preset", id: prompt.id })}
            />
          );
        })}
      </ListGroup>
      <Text variant="hint">{t("presetsHint")}</Text>
      {characterScopes.length > 0 && (
        <ListGroup label={t("characters")}>
          {characterScopes.map((item) => (
            <ListRowButton
              key={item.scope.id}
              icon={<User size={icon.lg} />}
              title={<ScopeTitle scope={item.scope} />}
              description={t("count", { count: item.scripts })}
              onClick={() => setOpen(item.scope)}
            />
          ))}
        </ListGroup>
      )}
    </Page>
  );
}
