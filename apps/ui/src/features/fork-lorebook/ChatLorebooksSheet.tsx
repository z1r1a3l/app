// Fork (z1r1a3l): lorebooks of this chat, one tap from the chat header, with entry
// generation and updates from chat history (backend job: lorebook_entry_draft).
import { useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { ArrowLeft, BookOpen, ExternalLink, FilePlus2, ListChecks, Plus, RefreshCw, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Button,
  EmptyState,
  icon,
  IconButton,
  ListGroup,
  ListRowButton,
  LoadingState,
  Progress,
  Sheet,
  toast,
} from "@lettuceai/crisp";
import type { LorebookEntryInput_Serialize, LorebookEntryView, LorebookView_Serialize, TimelineMessage } from "@/api/generated/bindings";
import { isApiFailure } from "@/api/client";
import { queryKeys } from "@/api/query-keys";
import { useApi } from "@/api/use-api";
import { lorebooksQuery } from "@/entities/lorebook";
import { LorebookEntrySheet, lorebookBookKey, lorebookBookQuery } from "@/features/library";
import { FailureState } from "@/shared/ui/FailureState";
import {
  activeLorebooksQuery,
  applyRequest,
  changes,
  chatMemoryQuery,
  DraftJobError,
  fillTemplate,
  proposalOf,
  runEntryDraft,
  type Proposal,
} from "./data";
import { DraftForm, type DraftPlan, type FormMode } from "./DraftForm";
import { ProposalReview, type Failed, type Unchanged } from "./ProposalReview";

type Screen =
  | { kind: "home" }
  | { kind: "book"; bookId: string }
  | { kind: "form"; mode: FormMode; bookId: string | null; entryIds: string[] }
  | { kind: "running"; done: number; total: number; current: string | null }
  | { kind: "none"; reason: string | null; plan: DraftPlan }
  | { kind: "proposals"; bookId: string; proposals: Proposal[]; unchanged: Unchanged[]; failed: Failed[] };

interface Editing {
  book: LorebookView_Serialize;
  entry: LorebookEntryView | null;
  initial?: Partial<LorebookEntryInput_Serialize>;
  fromProposal?: string;
}

export function ChatLorebooksSheet({
  open,
  onClose,
  conversationId,
  characterId,
  messages,
  phone,
}: {
  open: boolean;
  onClose: () => void;
  conversationId: string;
  characterId: string | null;
  messages: readonly TimelineMessage[];
  phone: boolean;
}) {
  const { t } = useTranslation("fork-lorebook");
  const { t: creation } = useTranslation("lorebook-create");
  const { t: common } = useTranslation();
  const api = useApi();
  const client = useQueryClient();
  const navigate = useNavigate();
  const direct = characterId !== null;
  const [screen, setScreen] = useState<Screen>({ kind: "home" });
  const [editing, setEditing] = useState<Editing | null>(null);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [applying, setApplying] = useState(false);
  const run = useRef<AbortController | null>(null);

  const active = useQuery({ ...activeLorebooksQuery(api, conversationId), enabled: open });
  const all = useQuery({ ...lorebooksQuery(api), enabled: open });
  const memory = useQuery({ ...chatMemoryQuery(api, conversationId), enabled: open && direct });
  const books = useMemo(() => {
    const byId = new Map((all.data ?? []).map((book) => [book.id, book]));
    return (active.data ?? []).flatMap((id) => (byId.has(id) ? [byId.get(id)!] : []));
  }, [active.data, all.data]);
  const bookScreenId = screen.kind === "book" ? screen.bookId : null;
  const book = useQuery({ ...lorebookBookQuery(api, bookScreenId ?? ""), enabled: open && bookScreenId !== null });

  const running = screen.kind === "running";
  const close = () => {
    if (running) return;
    onClose();
  };
  const home = () => setScreen({ kind: "home" });
  const fail = (error: unknown) => {
    const code = error instanceof DraftJobError ? error.code : null;
    if (code === "cancelled") return;
    if (code) toast.error(creation(`jobErrors.${code}`, { defaultValue: creation("jobErrors.unknown") }));
    else toast.error(common(`errors.code.${isApiFailure(error) ? error.code : "internal"}`));
  };
  const freshBook = (bookId: string) =>
    client.fetchQuery({ ...lorebookBookQuery(api, bookId), staleTime: 0 });

  const start = async (plan: DraftPlan) => {
    const abort = new AbortController();
    run.current = abort;
    const base = {
      conversation_id: conversationId,
      lorebook_id: plan.bookId,
      source: plan.source,
      message_ids: plan.messageIds,
      memory_ids: plan.memoryIds,
      use_summary: plan.useSummary,
    };
    try {
      const target = await freshBook(plan.bookId);
      if (plan.mode === "create") {
        setScreen({ kind: "running", done: 0, total: 1, current: null });
        const outcome = await runEntryDraft(api, { ...base, direction: plan.direction.trim() || null, force: plan.force }, abort.signal);
        if (outcome.kind === "none") {
          setScreen({ kind: "none", reason: outcome.reason, plan });
          return;
        }
        home();
        setEditing({
          book: target,
          entry: null,
          initial: {
            title: outcome.draft.title,
            content: outcome.draft.content,
            keywords: outcome.draft.keywords,
            always_active: outcome.draft.always_active,
          },
        });
        return;
      }
      const entries = plan.mode === "update" ? target.entries.filter((entry) => plan.entryIds.includes(entry.id)) : target.entries;
      const proposals: Proposal[] = [];
      const unchanged: Unchanged[] = [];
      const failed: Failed[] = [];
      for (const [index, entry] of entries.entries()) {
        if (abort.signal.aborted) break;
        setScreen({ kind: "running", done: index, total: entries.length, current: entry.title || t("untitled") });
        try {
          const outcome = await runEntryDraft(
            api,
            { ...base, direction: fillTemplate(plan.template, entry, plan.direction), force: plan.force },
            abort.signal,
          );
          if (outcome.kind === "none") unchanged.push({ entry, reason: outcome.reason });
          else {
            const proposal = proposalOf(entry, outcome.draft);
            if (changes(proposal)) proposals.push(proposal);
            else unchanged.push({ entry, reason: null });
          }
        } catch (error) {
          if (error instanceof DraftJobError && error.code === "cancelled") break;
          failed.push({ entry, code: error instanceof DraftJobError ? error.code : isApiFailure(error) ? error.code : "unknown" });
        }
      }
      setSelected(new Set(proposals.map((proposal) => proposal.entry.id)));
      setScreen({ kind: "proposals", bookId: plan.bookId, proposals, unchanged, failed });
    } catch (error) {
      fail(error);
      setScreen({ kind: "form", mode: plan.mode, bookId: plan.bookId, entryIds: plan.entryIds });
    } finally {
      if (run.current === abort) run.current = null;
    }
  };

  const apply = async () => {
    if (screen.kind !== "proposals") return;
    const accepted = screen.proposals.filter((proposal) => selected.has(proposal.entry.id));
    if (accepted.length === 0) return;
    setApplying(true);
    try {
      const current = await freshBook(screen.bookId);
      const view = await api.call("lorebookEntriesMutate", applyRequest(current, accepted));
      client.setQueryData(lorebookBookKey(screen.bookId), view);
      await client.invalidateQueries({ queryKey: queryKeys.lorebooks.all });
      toast.success(t("review.applied", { count: accepted.length }));
      const rest = screen.proposals.filter((proposal) => !selected.has(proposal.entry.id));
      if (rest.length === 0) home();
      else setScreen({ ...screen, proposals: rest });
      setSelected(new Set());
    } catch (error) {
      fail(error);
    } finally {
      setApplying(false);
    }
  };

  const dropProposal = (entryId: string) => {
    if (screen.kind !== "proposals") return;
    setScreen({ ...screen, proposals: screen.proposals.filter((proposal) => proposal.entry.id !== entryId) });
    setSelected((value) => {
      const next = new Set(value);
      next.delete(entryId);
      return next;
    });
  };

  let title = t("title");
  let body;
  let footer = null;
  if (screen.kind === "home") {
    body =
      active.isPending || all.isPending ? (
        <LoadingState label={t("loading")} />
      ) : active.isError || all.isError ? (
        <FailureState error={active.error ?? all.error} onRetry={() => void (active.isError ? active.refetch() : all.refetch())} />
      ) : (
        <div className="space-y-4 pb-1">
          {direct ? (
            <ListGroup label={t("home.fromChat")}>
              <ListRowButton
                icon={<Sparkles size={icon.md} />}
                tone="accent"
                title={t("home.create")}
                description={t("home.createHint")}
                disabled={books.length === 0}
                onClick={() => setScreen({ kind: "form", mode: "create", bookId: null, entryIds: [] })}
              />
              <ListRowButton
                icon={<RefreshCw size={icon.md} />}
                title={t("home.update")}
                description={t("home.updateHint")}
                disabled={books.length === 0}
                onClick={() => setScreen({ kind: "form", mode: "update", bookId: null, entryIds: [] })}
              />
              <ListRowButton
                icon={<ListChecks size={icon.md} />}
                title={t("home.review")}
                description={t("home.reviewHint")}
                disabled={books.length === 0}
                onClick={() => setScreen({ kind: "form", mode: "review", bookId: null, entryIds: [] })}
              />
            </ListGroup>
          ) : (
            <p className="text-sm text-fg-3">{t("home.groupOnly")}</p>
          )}
          <ListGroup label={t("home.books")}>
            {books.length === 0 ? (
              <EmptyState size="sm" icon={<BookOpen size={icon.lg} />} title={t("home.none")} description={t("home.noneHint")} />
            ) : (
              books.map((item) => (
                <ListRowButton
                  key={item.id}
                  icon={<BookOpen size={icon.md} />}
                  title={item.name}
                  chevron
                  onClick={() => setScreen({ kind: "book", bookId: item.id })}
                />
              ))
            )}
          </ListGroup>
        </div>
      );
  } else if (screen.kind === "book") {
    title = book.data?.lorebook.name ?? t("title");
    body = book.isPending ? (
      <LoadingState label={t("loading")} />
    ) : book.isError ? (
      <FailureState error={book.error} onRetry={() => void book.refetch()} />
    ) : (
      <div className="space-y-4 pb-1">
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" size="sm" leading={<Plus size={icon.sm} />} onClick={() => setEditing({ book: book.data, entry: null })}>
            {t("book.add")}
          </Button>
          {direct && (
            <Button
              variant="secondary"
              size="sm"
              leading={<FilePlus2 size={icon.sm} />}
              onClick={() => setScreen({ kind: "form", mode: "create", bookId: screen.bookId, entryIds: [] })}
            >
              {t("book.generate")}
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            leading={<ExternalLink size={icon.sm} />}
            onClick={() => {
              onClose();
              void navigate({ to: "/library/lorebooks/$lorebookId", params: { lorebookId: screen.bookId } });
            }}
          >
            {t("book.openEditor")}
          </Button>
        </div>
        {book.data.entries.length === 0 ? (
          <EmptyState size="sm" title={t("book.empty")} />
        ) : (
          <ListGroup label={t("book.entries", { count: book.data.entries.length })}>
            {book.data.entries.map((entry) => (
              <ListRowButton
                key={entry.id}
                title={entry.title || t("untitled")}
                description={<span className="line-clamp-2">{entry.content}</span>}
                className={entry.enabled ? undefined : "opacity-60"}
                chevron
                onClick={() => setEditing({ book: book.data, entry })}
              />
            ))}
          </ListGroup>
        )}
      </div>
    );
  } else if (screen.kind === "form") {
    title = t(`form.title.${screen.mode}`);
    body = (
      <DraftForm
        key={`${screen.mode}-${screen.bookId ?? ""}`}
        api={api}
        mode={screen.mode}
        books={books}
        messages={messages}
        memory={memory.data}
        initialBookId={screen.bookId}
        initialEntryIds={screen.entryIds}
        onSubmit={(plan) => void start(plan)}
      />
    );
  } else if (screen.kind === "running") {
    title = t("running.title");
    body = (
      <div className="space-y-4 py-4 text-center">
        <LoadingState label={t("loading")} />
        {screen.total > 1 && (
          <>
            <Progress value={Math.round((screen.done / screen.total) * 100)} />
            <p className="text-sm text-fg-2">{t("running.progress", { done: screen.done, total: screen.total })}</p>
          </>
        )}
        {screen.current && <p className="truncate text-sm text-fg-3">{t("running.current", { title: screen.current })}</p>}
        <p className="text-xs text-fg-3">{t("running.hint")}</p>
      </div>
    );
    footer = (
      <Button variant="ghost" block onClick={() => run.current?.abort()}>
        {t("running.cancel")}
      </Button>
    );
  } else if (screen.kind === "none") {
    title = t("none.title");
    body = (
      <div className="space-y-3 pb-1">
        <p className="text-sm text-fg-2">{t("none.body")}</p>
        {screen.reason && <p className="rounded-xl bg-fill p-3 text-sm text-fg-2">{screen.reason}</p>}
      </div>
    );
    footer = (
      <div className="flex gap-2">
        <Button variant="ghost" className="flex-1" onClick={() => setScreen({ kind: "form", mode: "create", bookId: screen.plan.bookId, entryIds: [] })}>
          {t("none.back")}
        </Button>
        <Button variant="primary" className="flex-1" onClick={() => void start({ ...screen.plan, force: true })}>
          {t("none.force")}
        </Button>
      </div>
    );
  } else {
    title = t("review.title");
    body = (
      <ProposalReview
        proposals={screen.proposals}
        selected={selected}
        unchanged={screen.unchanged}
        failed={screen.failed}
        busy={applying}
        onToggle={(id, on) =>
          setSelected((value) => {
            const next = new Set(value);
            if (on) next.add(id);
            else next.delete(id);
            return next;
          })
        }
        onReject={dropProposal}
        onEdit={(proposal) =>
          void freshBook(screen.bookId)
            .then((current) =>
              setEditing({
                book: current,
                entry: current.entries.find((entry) => entry.id === proposal.entry.id) ?? proposal.entry,
                initial: { title: proposal.title, content: proposal.content, keywords: proposal.keywords },
                fromProposal: proposal.entry.id,
              }),
            )
            .catch(fail)
        }
      />
    );
    footer = (
      <div className="flex gap-2">
        <Button variant="ghost" className="flex-1" disabled={applying} onClick={home}>
          {t("review.discard")}
        </Button>
        <Button
          variant="primary"
          className="flex-1"
          loading={applying}
          disabled={selected.size === 0}
          onClick={() => void apply()}
        >
          {t("review.apply", { count: selected.size })}
        </Button>
      </div>
    );
  }

  return (
    <>
      <Sheet
        open={open && editing === null}
        onClose={close}
        dismissible={!running && !applying}
        placement={phone ? "bottom" : "side"}
        size="lg"
        title={title}
        leftAction={
          screen.kind !== "home" && !running ? (
            <IconButton label={t("back")} size="sm" disabled={applying} onClick={home}>
              <ArrowLeft size={icon.md} />
            </IconButton>
          ) : undefined
        }
        footer={footer}
      >
        {body}
      </Sheet>
      {editing && (
        <LorebookEntrySheet
          key={`${editing.book.lorebook.id}-${editing.entry?.id ?? "new"}`}
          book={editing.book}
          entry={editing.entry}
          {...(editing.initial ? { initial: editing.initial } : {})}
          open
          phone={phone}
          onClose={() => setEditing(null)}
          onSaved={() => {
            if (editing.fromProposal) dropProposal(editing.fromProposal);
            toast.success(t("saved"));
          }}
        />
      )}
    </>
  );
}
