import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryHistory } from "@tanstack/react-router";
import { toast } from "@lettuceai/crisp";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { LorebookEntriesMutateRequest, LorebookEntryDraftRequest, LorebookView_Serialize } from "@/api/generated/bindings";
import { createMockTransport, mockAppStatus, type MockCallHandlers } from "@/api/mock-transport";
import { App } from "@/app/bootstrap/App";
import { createAppRuntime } from "@/app/bootstrap/runtime";
import { createTestI18n } from "@/shared/testing/i18n";
import { defaultTemplates, fillTemplate, recentMessageIds } from "./data";
import { fixtureBook, fixtureEntry, fixtureJob, fixtureMemory, fixtureMessage, fixtureSettings, fixtureView } from "./fixtures";

vi.mock("@/api/window-controls", () => ({ createWindowController: vi.fn().mockResolvedValue(null) }));
vi.mock("@/api/haptics", () => ({ typingHaptic: vi.fn().mockResolvedValue(undefined) }));

afterEach(() => {
  toast.dismiss();
  cleanup();
  localStorage.clear();
});

const ok = <T,>(data: T) => ({ status: "ok" as const, data });

const messages = [
  fixtureMessage("u1", "user", "Who runs the Salt Anchor now?"),
  fixtureMessage("a1", "assistant", "Mara bought the tavern from old Bren last winter."),
  fixtureMessage("u2", "user", "And the cellar?"),
  fixtureMessage("a2", "assistant", "She sealed the cellar after the flood."),
];

function backend(drafts: Record<string, Parameters<typeof fixtureJob>[1]> = {}) {
  let world: LorebookView_Serialize = fixtureBook("world", "Harbour world", [
    fixtureEntry("e1", "Salt Anchor", "A tavern run by Bren.", ["salt anchor", "tavern"]),
    fixtureEntry("e2", "Lighthouse", "Abandoned lighthouse on the cape.", ["lighthouse"]),
  ]);
  const terms = fixtureBook("terms", "Terminology", [fixtureEntry("t1", "Knot", "Unit of speed.", ["knot"])]);
  const draftRequests: LorebookEntryDraftRequest[] = [];
  const writes: LorebookEntriesMutateRequest[] = [];
  let jobs = 0;
  const calls: MockCallHandlers = {
    appStatus: () => ok({ ...mockAppStatus, ui_state: { onboarding: { completed: true } } }),
    conversationOpen: () => ok(fixtureView(messages)),
    conversationSettingsGet: () => ok(fixtureSettings),
    modelsList: () => ok({ models: [], default_model_id: null, revision: 1 }),
    providerAccountsList: () => ok([]),
    memoryGet: () => ok(fixtureMemory([{ id: "m1", text: "Mara owns the Salt Anchor." }], "Hunter arrived at the harbour.")),
    llmMetricsForMessage: () => ok(null),
    messageUsage: () => ok(null),
    charactersList: () => ok({ items: [], next_cursor: null }),
    conversationsLatestByCharacter: () => ok({ items: [], next_cursor: null }),
    conversationsList: () => ok({ items: [], next_cursor: null }),
    lorebookTriggerPreview: () => ok({ entries: [], skipped: [], scan_depth: 4, scanned_message_ids: [], active_lorebook_ids: ["world", "terms"] }),
    lorebooksList: () => ok({ items: [world.lorebook, terms.lorebook] as never, next_cursor: null }),
    lorebookGet: ({ lorebook_id }) => ok(lorebook_id === "terms" ? terms : world),
    lorebookEntryDraft: (request) => {
      draftRequests.push(request);
      jobs += 1;
      return ok({ job_id: `job-${jobs}` });
    },
    lorebookEntriesMutate: (request) => {
      writes.push(request);
      world = { ...world, lorebook: { ...world.lorebook, revision: world.lorebook.revision + 1 } };
      return ok(world);
    },
  };
  const streams = {
    jobWatch: (args: unknown) => {
      const job_id = (Array.isArray(args) ? args[0] : args).job_id as string;
      const request = draftRequests[Number(job_id.split("-")[1]) - 1]!;
      const key = request.direction?.includes("Entry title: ") ? /Entry title: (.*)/.exec(request.direction)![1]! : "create";
      return ok(fixtureJob(job_id, drafts[key] ?? { type: "lorebook_no_entry", reason: "Nothing new." }));
    },
  };
  return { calls, streams, draftRequests, writes };
}

async function open(store: ReturnType<typeof backend>) {
  const runtime = await createAppRuntime({
    transport: createMockTransport({ calls: store.calls, streams: store.streams as never }),
    history: createMemoryHistory({ initialEntries: ["/chat/c1"] }),
    i18n: await createTestI18n("en"),
  });
  await act(async () => {
    render(<App runtime={runtime} />);
  });
  fireEvent.click(await screen.findByRole("button", { name: "Open chat lorebooks" }));
  return within(await screen.findByRole("dialog"));
}

async function pickBook(sheet: ReturnType<typeof within>, name: string) {
  fireEvent.click(sheet.getByRole("combobox", { name: "Lorebook" }));
  fireEvent.click(await screen.findByRole("option", { name }));
}

describe("chat lorebook shortcut", () => {
  it("lists the chat's active lorebooks and their entries", async () => {
    const sheet = await open(backend());
    expect(await sheet.findByText("Harbour world")).toBeTruthy();
    expect(sheet.getByText("Terminology")).toBeTruthy();
    fireEvent.click(sheet.getByText("Harbour world"));
    expect(await sheet.findByText("Salt Anchor")).toBeTruthy();
    expect(sheet.getByText("2 entries")).toBeTruthy();
  });

  it("drafts a new entry from recent messages and opens it for review", async () => {
    const store = backend({
      create: { type: "lorebook_entry_draft", draft: { title: "Mara", content: "Owner of the Salt Anchor.", keywords: ["mara"], always_active: false } },
    });
    const sheet = await open(store);
    fireEvent.click(await sheet.findByText("New entry from chat"));
    const submit = await sheet.findByRole("button", { name: "Generate draft" });
    expect((submit as HTMLButtonElement).disabled).toBe(true);
    await pickBook(sheet, "Harbour world");
    fireEvent.click(submit);
    const editor = await screen.findByDisplayValue("Mara");
    expect(editor).toBeTruthy();
    expect(store.draftRequests[0]).toMatchObject({
      conversation_id: "c1", lorebook_id: "world", source: "messages", message_ids: ["u1", "a1", "u2", "a2"], memory_ids: [], force: false,
    });
    fireEvent.click(screen.getByRole("button", { name: "Save entry" }));
    await waitFor(() => expect(store.writes).toHaveLength(1));
    expect(store.writes[0]!.mutations[0]).toMatchObject({ type: "add", entry: { title: "Mara", content: "Owner of the Salt Anchor." } });
  });

  it("reviews only the chosen lorebook and writes nothing until changes are applied", async () => {
    const store = backend({
      "Salt Anchor": { type: "lorebook_entry_draft", draft: { title: "Salt Anchor", content: "A tavern run by Mara since last winter.", keywords: ["salt anchor", "tavern", "mara"], always_active: false } },
    });
    const sheet = await open(store);
    fireEvent.click(await sheet.findByText("Review a lorebook"));
    const submit = await sheet.findByRole("button", { name: "Start review" });
    expect((submit as HTMLButtonElement).disabled).toBe(true);
    await pickBook(sheet, "Harbour world");
    fireEvent.click(submit);
    expect(await sheet.findByText("1 entry needs changes. Tick what to keep.")).toBeTruthy();
    expect(store.draftRequests.map((request) => request.lorebook_id)).toEqual(["world", "world"]);
    expect(store.draftRequests.every((request) => request.force === false)).toBe(true);
    expect(store.writes).toHaveLength(0);
    fireEvent.click(sheet.getByRole("button", { name: "Apply 1" }));
    await waitFor(() => expect(store.writes).toHaveLength(1));
    expect(store.writes[0]).toMatchObject({
      lorebook_id: "world",
      mutations: [{ type: "update", entry_id: "e1", entry: { content: "A tavern run by Mara since last winter.", keywords: ["salt anchor", "tavern", "mara"] } }],
    });
  });

  it("rejected proposals are never written", async () => {
    const store = backend({
      "Salt Anchor": { type: "lorebook_entry_draft", draft: { title: "Salt Anchor", content: "Changed.", keywords: [], always_active: false } },
      Lighthouse: { type: "lorebook_entry_draft", draft: { title: "Lighthouse", content: "Relit by Mara.", keywords: [], always_active: false } },
    });
    const sheet = await open(store);
    fireEvent.click(await sheet.findByText("Review a lorebook"));
    await pickBook(sheet, "Harbour world");
    fireEvent.click(sheet.getByRole("button", { name: "Start review" }));
    expect(await sheet.findByText("2 entries need changes. Tick what to keep.")).toBeTruthy();
    fireEvent.click(sheet.getAllByRole("button", { name: "Reject" })[0]!);
    fireEvent.click(sheet.getByRole("button", { name: "Apply 1" }));
    await waitFor(() => expect(store.writes).toHaveLength(1));
    expect(store.writes[0]!.mutations).toHaveLength(1);
    expect(store.writes[0]!.mutations[0]).toMatchObject({ entry_id: "e2" });
  });
});

describe("fork lorebook helpers", () => {
  it("picks the newest text messages and fills the update instruction", () => {
    expect(recentMessageIds(messages, 2)).toEqual(["u2", "a2"]);
    const text = fillTemplate(defaultTemplates.single, fixtureEntry("e", "Gate", "Old gate.", ["gate"]), "");
    expect(text).toContain("Entry title: Gate");
    expect(text).toContain("Old gate.");
    expect(text).toContain("User direction: (none)");
  });
});
