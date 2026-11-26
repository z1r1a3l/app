// Fork (z1r1a3l): sample data for the chat lorebook shortcut tests and screenshot harness.
import type {
  ConversationSettingsView,
  ConversationView,
  JobView,
  LorebookEntryView,
  LorebookView_Serialize,
  MemoryView,
  TimelineMessage,
} from "@/api/generated/bindings";

export function fixtureMessage(id: string, role: TimelineMessage["role"], text: string): TimelineMessage {
  return {
    id,
    role,
    author_participant_id: role === "user" ? "pu" : "pc",
    parts: [{ type: "text", text }],
    reasoning: null,
    created_at: 1_700_000_000_000,
    effective_time: 1_700_000_000_000,
    candidate_index: null,
    candidate_count: 1,
    pinned: false,
    scene_image: null,
  };
}

export function fixtureEntry(id: string, title: string, content: string, keywords: string[]): LorebookEntryView {
  return {
    id,
    title,
    content,
    keywords,
    secondary_keywords: [],
    secondary_logic: "and_any",
    enabled: true,
    always_active: false,
    case_sensitive: false,
    keyword_mode: "literal",
    priority: 0,
    ordinal: 0,
    revision: 1,
    created_at: 1,
    updated_at: 1,
  };
}

export function fixtureBook(id: string, name: string, entries: LorebookEntryView[]): LorebookView_Serialize {
  return {
    lorebook: { id, name, status: "active", detection: "recent_messages", icon: null, revision: 3, created_at: 1, updated_at: 1 },
    entries,
  };
}

export function fixtureView(messages: TimelineMessage[], kind: "direct" | "group" = "direct"): ConversationView {
  return {
    id: "c1",
    origin_conversation_id: null,
    origin_message_id: null,
    kind,
    title: "Harbour",
    branch: { branch_id: "b", head_message_id: null },
    participants: [
      { id: "pu", role: "user", name: "Hunter", character_id: null, avatar: null },
      { id: "pc", role: "character", name: "Cassian", character_id: "ch1", avatar: null },
    ],
    messages: { items: messages, next_cursor: null },
    pending_turn_id: null,
    can_send: true,
    revision: 4,
    settings_revision: 2,
    archived: false,
    source: kind === "direct" ? { type: "direct", character_id: "ch1" } : { type: "group", group_id: "g1" },
    missing_models: [],
    memory_blocked: null,
  } as ConversationView;
}

export const fixtureSettings = {
  conversation_id: "c1",
  kind: "direct",
  title: "Harbour",
  revision: 4,
  settings_revision: 2,
  author_note: null,
  persona: { id: null, source: "app_default" },
  prompt: { id: null, source: "app_default" },
  lorebooks: { ids: ["terms"], source: "conversation" },
  model: { id: "m1", source: "character" },
  background: { asset: null, hidden: false, source: "character" },
  scene: { id: null, source: "app_default" },
  memory: { mode: "dynamic", source: "app_default" },
  disable_character_lorebooks: null,
  disable_character_notes: null,
  companion_mode: null,
  companion_personal_memories: null,
  companion_memory_write_back: null,
  speaker_selection: null,
  members: null,
  model_settings: { chat_parameters: {}, llama_cpp: {}, stable_diffusion: {} },
  time: { enabled: false, mode: "live", anchor_at: null, set_at: null, effective_at: 1_700_000_000_000 },
  companion: false,
} as unknown as ConversationSettingsView;

export function fixtureMemory(items: { id: string; text: string }[], summary: string | null): MemoryView {
  return {
    items: items.map((item, index) => ({
      id: item.id,
      short_id: `m${index}`,
      text: item.text,
      category: "plot_event",
      origin: "model",
      pinned: false,
      temperature: "hot",
      observed_at: null,
      observed_time_precision: null,
      token_count: null,
      cycle_label: null,
    })),
    allowed_categories: ["plot_event"],
    summary: summary === null ? null : ({ text: summary } as never),
    summary_parts: null,
    revision: 1,
    status: {
      run_mode: "auto",
      interval: 20,
      messages_since_last_cycle: 0,
      messages_until_next_cycle: 20,
      total_conversation_messages: 6,
      pending_approval_count: null,
      skipped: false,
      latest_cycle_status: null,
      latest_job_id: null,
      failure: null,
      paused_reason: null,
    },
  } as MemoryView;
}

export function fixtureJob(id: string, result: JobView["result"]): JobView {
  return {
    id,
    kind: "creation_run",
    subject: { kind: "model_profile", id: "model-1" },
    subject_detail: null,
    state: "succeeded",
    created_at: 1,
    updated_at: 1,
    progress: { current: 0, total: null, unit: null, label_code: null, bytes_per_second: null },
    failure: null,
    result,
  } as JobView;
}
