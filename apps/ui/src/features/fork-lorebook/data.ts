// Fork (z1r1a3l): data helpers for the chat lorebook shortcut.
import { queryOptions } from "@tanstack/react-query";
import type { ApiClient } from "@/api/client";
import type {
  LorebookEntriesMutateRequest,
  LorebookEntryDraftRequest,
  LorebookEntryDraftResult,
  LorebookEntryView,
  LorebookView_Serialize,
  TimelineMessage,
} from "@/api/generated/bindings";
import { queryKeys } from "@/api/query-keys";
import { watchJob } from "@/entities/job";

export type DraftSource = LorebookEntryDraftRequest["source"];

/** The lorebooks the next turn of this chat would read (character, persona and chat-level books). */
export function activeLorebooksQuery(api: ApiClient, conversationId: string) {
  return queryOptions({
    queryKey: [...queryKeys.lorebooks.all, "fork-chat-active", conversationId] as const,
    queryFn: async (): Promise<string[]> => {
      const preview = await api.call("lorebookTriggerPreview", {
        mode: "conversation",
        conversation_id: conversationId,
        composer_text: null,
        speaker_character_id: null,
      });
      return preview.active_lorebook_ids;
    },
  });
}

export function chatMemoryQuery(api: ApiClient, conversationId: string) {
  return queryOptions({
    queryKey: queryKeys.memory.detail(conversationId),
    queryFn: () => api.call("memoryGet", { conversation_id: conversationId }),
  });
}

/** Ids of the newest `count` user/character messages that carry text, oldest first. */
export function recentMessageIds(messages: readonly TimelineMessage[], count: number | "all"): string[] {
  const usable = messages.filter(
    (message) =>
      (message.role === "user" || message.role === "assistant") &&
      message.parts.some((part) => part.type === "text" && part.text.trim() !== ""),
  );
  const picked = count === "all" ? usable : usable.slice(-count);
  return picked.map((message) => message.id);
}

export type DraftOutcome =
  | { kind: "draft"; draft: LorebookEntryDraftResult }
  | { kind: "none"; reason: string | null };

export class DraftJobError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(code);
    this.code = code;
  }
}

/** Runs one lorebook_entry_draft job to its end. Nothing is written to the book. */
export async function runEntryDraft(
  api: ApiClient,
  request: Omit<LorebookEntryDraftRequest, "client_operation_id">,
  signal: AbortSignal,
): Promise<DraftOutcome> {
  const accepted = await api.call("lorebookEntryDraft", { ...request, client_operation_id: crypto.randomUUID() });
  if (signal.aborted) {
    void api.call("jobCancel", { job_id: accepted.job_id }).catch(() => undefined);
    throw new DraftJobError("cancelled");
  }
  const onAbort = () => void api.call("jobCancel", { job_id: accepted.job_id }).catch(() => undefined);
  signal.addEventListener("abort", onAbort, { once: true });
  try {
    const job = await watchJob(api, accepted.job_id, () => undefined, signal);
    if (job.state === "succeeded" && job.result?.type === "lorebook_entry_draft") return { kind: "draft", draft: job.result.draft };
    if (job.state === "succeeded" && job.result?.type === "lorebook_no_entry") return { kind: "none", reason: job.result.reason };
    throw new DraftJobError(job.state === "cancelled" ? "cancelled" : job.failure?.code ?? "unknown");
  } finally {
    signal.removeEventListener("abort", onAbort);
  }
}

// ---- Update mode: the existing entry writer is steered by an editable instruction. ----

export type TemplateKind = "single" | "review";

export const defaultTemplates: Record<TemplateKind, string> = {
  single: [
    "UPDATE MODE. Do not create a new entry. Revise the existing lorebook entry below using the selected chat history and memories.",
    "- Keep facts that are still true; fix facts the chat has changed; add newly established durable canon about this subject.",
    "- Do not add facts about other subjects. Do not drop valid details.",
    "- Keep the same title unless it is wrong.",
    "- Return the COMPLETE revised entry (full content, not a diff) and the full keyword list.",
    "- Write in the same language as the existing entry.",
    "",
    "Entry title: {{title}}",
    "Entry keywords: {{keywords}}",
    "Entry content:",
    "<<<",
    "{{content}}",
    ">>>",
    "",
    "User direction: {{direction}}",
  ].join("\n"),
  review: [
    "REVIEW MODE. Do not create a new entry. Check the existing lorebook entry below against the selected chat history and memories.",
    "- If the entry is still accurate and the chat established nothing new about its subject, use the no_entry tool with a one-line reason.",
    "- Otherwise return the COMPLETE revised entry (full content, not a diff) with the same title and the full keyword list.",
    "- Keep facts that are still true; fix outdated facts; add only durable canon about this subject.",
    "- Write in the same language as the existing entry.",
    "",
    "Entry title: {{title}}",
    "Entry keywords: {{keywords}}",
    "Entry content:",
    "<<<",
    "{{content}}",
    ">>>",
    "",
    "User direction: {{direction}}",
  ].join("\n"),
};

const templateKey = (kind: TemplateKind) => `lettuce.fork.lorebookUpdater.${kind}`;

export function loadTemplate(kind: TemplateKind): string {
  try {
    return localStorage.getItem(templateKey(kind)) ?? defaultTemplates[kind];
  } catch {
    return defaultTemplates[kind];
  }
}

export function saveTemplate(kind: TemplateKind, text: string): void {
  try {
    if (text.trim() === "" || text === defaultTemplates[kind]) localStorage.removeItem(templateKey(kind));
    else localStorage.setItem(templateKey(kind), text);
  } catch {
    // storage unavailable: the edit lasts for this session only
  }
}

export function fillTemplate(template: string, entry: LorebookEntryView, direction: string): string {
  const values: Record<string, string> = {
    title: entry.title || "(untitled)",
    keywords: entry.keywords.join(", ") || "(none)",
    content: entry.content,
    direction: direction.trim() || "(none)",
  };
  return template.replace(/\{\{(title|keywords|content|direction)\}\}/g, (_, key: string) => values[key] ?? "");
}

export interface Proposal {
  entry: LorebookEntryView;
  title: string;
  content: string;
  keywords: string[];
}

export function proposalOf(entry: LorebookEntryView, draft: LorebookEntryDraftResult): Proposal {
  return {
    entry,
    title: draft.title.trim() || entry.title,
    content: draft.content.trim(),
    keywords: draft.keywords.length > 0 ? draft.keywords : entry.keywords,
  };
}

/** Whether a proposal actually changes the entry. */
export function changes(proposal: Proposal): boolean {
  const { entry } = proposal;
  return (
    proposal.title !== entry.title ||
    proposal.content.trim() !== entry.content.trim() ||
    proposal.keywords.join("\n") !== entry.keywords.join("\n")
  );
}

/** One revision-checked write that applies every accepted proposal. */
export function applyRequest(book: LorebookView_Serialize, accepted: readonly Proposal[]): LorebookEntriesMutateRequest {
  return {
    client_operation_id: crypto.randomUUID(),
    lorebook_id: book.lorebook.id,
    expected_revision: book.lorebook.revision,
    mutations: accepted.map((proposal) => {
      const current = book.entries.find((entry) => entry.id === proposal.entry.id) ?? proposal.entry;
      return {
        type: "update" as const,
        entry_id: current.id,
        entry: {
          title: proposal.title,
          enabled: current.enabled,
          always_active: current.always_active,
          keywords: proposal.keywords,
          secondary_keywords: current.secondary_keywords,
          secondary_logic: current.secondary_logic,
          case_sensitive: current.case_sensitive,
          keyword_mode: current.keyword_mode,
          content: proposal.content,
          priority: current.priority,
        },
      };
    }),
  };
}

/** Words in any script (Cyrillic included), runs of spaces and punctuation as separate tokens. */
export function tokens(text: string): string[] {
  return text.match(/[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]+/gu) ?? [];
}
