// Fork (z1r1a3l): the "from chat history" form shared by create, update and review.
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  Button,
  Checkbox,
  Chip,
  Collapsible,
  Field,
  ListGroup,
  ListRow,
  Segmented,
  SelectMenu,
  Switch,
  Textarea,
} from "@lettuceai/crisp";
import type { ApiClient } from "@/api/client";
import type { LorebookSummary, MemoryView, TimelineMessage } from "@/api/generated/bindings";
import { lorebookBookQuery } from "@/features/library";
import { SelectMenuField } from "@/shared/ui/SelectMenuField";
import { defaultTemplates, loadTemplate, recentMessageIds, saveTemplate, type DraftSource, type TemplateKind } from "./data";

export type FormMode = "create" | "update" | "review";

export interface DraftPlan {
  mode: FormMode;
  bookId: string;
  entryIds: string[];
  source: DraftSource;
  messageIds: string[];
  memoryIds: string[];
  useSummary: boolean;
  direction: string;
  force: boolean;
  template: string;
}

const counts = [10, 20, 50, "all"] as const;
type Count = (typeof counts)[number];

export function DraftForm({
  api,
  mode,
  books,
  messages,
  memory,
  initialBookId,
  initialEntryIds,
  onSubmit,
}: {
  api: ApiClient;
  mode: FormMode;
  books: readonly LorebookSummary[];
  messages: readonly TimelineMessage[];
  memory: MemoryView | undefined;
  initialBookId: string | null;
  initialEntryIds: readonly string[];
  onSubmit: (plan: DraftPlan) => void;
}) {
  const { t } = useTranslation("fork-lorebook");
  const [bookId, setBookId] = useState<string | null>(
    initialBookId ?? (mode !== "review" && books.length === 1 ? books[0]!.id : null),
  );
  const [entryIds, setEntryIds] = useState<string[]>([...initialEntryIds]);
  const [source, setSource] = useState<DraftSource>("messages");
  const [count, setCount] = useState<Count>(20);
  const memories = memory?.items ?? [];
  const [skippedMemories, setSkippedMemories] = useState<ReadonlySet<string>>(new Set());
  const hasSummary = Boolean(memory?.summary);
  const [useSummary, setUseSummary] = useState(true);
  const [direction, setDirection] = useState("");
  const [force, setForce] = useState(false);
  const templateKind: TemplateKind = mode === "review" ? "review" : "single";
  const [template, setTemplate] = useState(() => loadTemplate(templateKind));
  const book = useQuery({ ...lorebookBookQuery(api, bookId ?? ""), enabled: bookId !== null && mode === "update" });

  const messageIds = source === "memory" ? [] : recentMessageIds(messages, count);
  const memoryIds = source === "messages" ? [] : memories.filter((item) => !skippedMemories.has(item.id)).map((item) => item.id);
  const summaryOn = source !== "messages" && hasSummary && useSummary;
  const sourceReady =
    source === "messages" ? messageIds.length > 0 : source === "memory" ? memoryIds.length > 0 || summaryOn : messageIds.length + memoryIds.length > 0 || summaryOn;
  const ready = bookId !== null && sourceReady && (mode !== "update" || entryIds.length > 0);

  return (
    <div className="space-y-5 pb-1">
      <p className="text-sm text-fg-3">{t(`form.intro.${mode}`)}</p>
      <Field label={t("form.book")} hint={mode === "review" ? t("form.bookReviewHint") : undefined} required>
        <SelectMenuField
          aria-label={t("form.book")}
          placeholder={t("form.bookPlaceholder")}
          value={bookId}
          options={books.map((item) => ({ value: item.id, label: item.name }))}
          onChange={(value) => {
            setBookId(value);
            setEntryIds([]);
          }}
        />
      </Field>
      {mode === "update" && bookId !== null && (
        <Field label={t("form.entries")} hint={t("form.entriesHint")} required>
          <SelectMenu
            multiple
            className="w-full"
            placeholder={book.isPending ? t("loading") : t("form.entriesPlaceholder")}
            disabled={!book.data}
            value={entryIds}
            options={(book.data?.entries ?? []).map((entry) => ({ value: entry.id, label: entry.title || t("untitled") }))}
            onChange={setEntryIds}
          />
        </Field>
      )}
      <Field label={t("form.source")}>
        <Segmented
          block
          aria-label={t("form.source")}
          value={source}
          onChange={setSource}
          options={[
            { value: "messages", label: t("source.messages") },
            { value: "memory", label: t("source.memory") },
            { value: "mixed", label: t("source.mixed") },
          ]}
        />
      </Field>
      {source !== "memory" && (
        <Field label={t("form.messages")} hint={t("form.messagesHint", { count: messageIds.length })}>
          <div className="flex flex-wrap gap-2">
            {counts.map((value) => (
              <Chip key={value} selected={count === value} onClick={() => setCount(value)}>
                {value === "all" ? t("form.allLoaded") : t("form.lastN", { count: value })}
              </Chip>
            ))}
          </div>
        </Field>
      )}
      {source !== "messages" && (
        <ListGroup label={t("form.memory")}>
          <ListRow
            title={t("form.useSummary")}
            description={hasSummary ? t("form.useSummaryHint") : t("form.noSummary")}
            trailing={<Switch aria-label={t("form.useSummary")} checked={summaryOn} disabled={!hasSummary} onChange={setUseSummary} />}
          />
          {memories.length === 0 ? (
            <ListRow title={t("form.noMemories")} />
          ) : (
            <Collapsible
              variant="plain"
              title={t("form.memoriesPicked", { count: memoryIds.length, total: memories.length })}
            >
              <div className="space-y-2">
                {memories.map((item) => (
                  <Checkbox
                    key={item.id}
                    checked={!skippedMemories.has(item.id)}
                    onChange={(on) =>
                      setSkippedMemories((value) => {
                        const next = new Set(value);
                        if (on) next.delete(item.id);
                        else next.add(item.id);
                        return next;
                      })
                    }
                    label={<span className="line-clamp-2 text-sm">{item.text}</span>}
                  />
                ))}
              </div>
            </Collapsible>
          )}
        </ListGroup>
      )}
      <Field label={t("form.direction")} hint={t(`form.directionHint.${mode}`)}>
        <Textarea rows={3} value={direction} placeholder={t("form.directionPlaceholder")} onChange={(event) => setDirection(event.target.value)} />
      </Field>
      {mode === "create" && (
        <ListGroup>
          <ListRow
            title={t("form.force")}
            description={t("form.forceHint")}
            trailing={<Switch aria-label={t("form.force")} checked={force} onChange={setForce} />}
          />
        </ListGroup>
      )}
      {mode !== "create" && (
        <Collapsible variant="plain" title={t("form.instruction")} description={t("form.instructionHint")}>
          <div className="space-y-2">
            <Textarea rows={8} value={template} onChange={(event) => setTemplate(event.target.value)} />
            <Button variant="ghost" size="sm" disabled={template === defaultTemplates[templateKind]} onClick={() => setTemplate(defaultTemplates[templateKind])}>
              {t("form.instructionReset")}
            </Button>
          </div>
        </Collapsible>
      )}
      <Button
        variant="primary"
        block
        disabled={!ready}
        onClick={() => {
          if (bookId === null) return;
          if (mode !== "create") saveTemplate(templateKind, template);
          onSubmit({
            mode,
            bookId,
            entryIds,
            source,
            messageIds,
            memoryIds,
            useSummary: summaryOn,
            direction,
            force: mode === "update" ? true : mode === "review" ? false : force,
            template,
          });
        }}
      >
        {t(`form.submit.${mode}`)}
      </Button>
    </div>
  );
}
