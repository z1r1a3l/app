import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryHistory } from "@tanstack/react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DiscoverItemRequest, DiscoverSearchRequest } from "@/api/generated/bindings";
import { createMockTransport, type MockCallHandlers, mockAppStatus } from "@/api/mock-transport";
import { App } from "@/app/bootstrap/App";
import { createAppRuntime } from "@/app/bootstrap/runtime";
import { createTestI18n } from "@/shared/testing/i18n";
import { fixtureCard, fixtureDetail } from "./fixtures";

vi.mock("@/api/window-controls", () => ({ createWindowController: vi.fn().mockResolvedValue(null) }));

beforeEach(() => localStorage.clear());
afterEach(() => cleanup());

const ok = <T,>(data: T) => ({ status: "ok" as const, data });

const eira = fixtureCard("crystal/eira", "Eira the Witch", {
  tagline: "A lonely witch in a tower",
  creator: "crystal",
  tags: ["Fantasy", "Female"],
  nsfw: true,
  tokens: 2100,
  stats: { stars: 120, downloads: null, chats: 3400, messages: null, views: null, likes: null, rating: null },
  page_url: "https://chub.ai/characters/crystal/eira",
});
const robot = fixtureCard("anon/robot", "Robot Butler", { tagline: "Beep" });

async function open(calls: MockCallHandlers) {
  const runtime = await createAppRuntime({
    transport: createMockTransport({
      calls: {
        appStatus: () => ok({ ...mockAppStatus, ui_state: { onboarding: { completed: true } } }),
        conversationsList: () => ok({ items: [], next_cursor: null }),
        ...calls,
      },
    }),
    history: createMemoryHistory({ initialEntries: ["/discover"] }),
    i18n: await createTestI18n("en"),
  });
  await act(async () => {
    render(<App runtime={runtime} />);
  });
  return runtime;
}

it("searches the chosen hub with the NSFW toggle and switches sources", async () => {
  const requests: DiscoverSearchRequest[] = [];
  await open({
    discoverSearch: (request) => {
      requests.push(request);
      return ok({ items: request.source === "chub" ? [eira, robot] : [robot], next_cursor: null, total: 2 });
    },
  });
  expect(await screen.findByText("Eira the Witch")).toBeTruthy();
  expect(screen.getByText("18+", { selector: "span.rounded-md" })).toBeTruthy();
  expect(requests[0]).toMatchObject({ source: "chub", kind: "character", nsfw: true, nsfl: false, sort: "default", cursor: null });

  fireEvent.click(screen.getByRole("tab", { name: "Wyvern" }));
  await waitFor(() => expect(requests.at(-1)).toMatchObject({ source: "wyvern", sort: "total_likes" }));
  await waitFor(() => expect(screen.queryByText("Eira the Witch")).toBeNull());

  fireEvent.click(screen.getAllByRole("switch", { name: "Show adult content" })[0]!);
  await waitFor(() => expect(requests.at(-1)).toMatchObject({ source: "wyvern", nsfw: false }));
});

it("keeps Chub's NSFL toggle separate from 18+ and off by default", async () => {
  const requests: DiscoverSearchRequest[] = [];
  await open({
    discoverSearch: (request) => {
      requests.push(request);
      return ok({ items: [robot], next_cursor: null, total: 1 });
    },
  });
  expect(await screen.findByText("Robot Butler")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Tags" }));
  const sheet = within(await screen.findByRole("dialog"));
  const nsfl = sheet.getByRole("switch", { name: "Show NSFL content" });
  expect(nsfl.getAttribute("aria-checked")).toBe("false");
  fireEvent.click(nsfl);
  await waitFor(() => expect(requests.at(-1)).toMatchObject({ source: "chub", nsfw: true, nsfl: true }));
  expect(JSON.parse(localStorage.getItem("lettuce.fork.discover") ?? "{}")).toMatchObject({ nsfl: true });

  // Turning 18+ off also stops NSFL and locks its toggle.
  fireEvent.click(sheet.getByRole("switch", { name: "Show adult content" }));
  await waitFor(() => expect(requests.at(-1)).toMatchObject({ nsfw: false, nsfl: false }));
  expect(sheet.getByRole("switch", { name: "Show NSFL content" }).hasAttribute("disabled")).toBe(true);
});

it("hides the NSFL toggle for hubs without it and drops removed sources from saved prefs", async () => {
  localStorage.setItem("lettuce.fork.discover", JSON.stringify({ source: "janny", nsfw: true }));
  const requests: DiscoverSearchRequest[] = [];
  await open({
    discoverSearch: (request) => {
      requests.push(request);
      return ok({ items: [robot], next_cursor: null, total: 1 });
    },
  });
  await waitFor(() => expect(requests[0]).toMatchObject({ source: "chub", nsfl: false }));
  fireEvent.click(screen.getByRole("tab", { name: "Wyvern" }));
  fireEvent.click(screen.getByRole("button", { name: "Tags" }));
  const sheet = within(await screen.findByRole("dialog"));
  expect(sheet.queryByRole("switch", { name: "Show NSFL content" })).toBeNull();
});

it("shows details and hands the downloaded card to the import sheet", async () => {
  const downloads: DiscoverItemRequest[] = [];
  await open({
    discoverSearch: () => ok({ items: [eira], next_cursor: null, total: 1 }),
    discoverDetail: () => ok(fixtureDetail(eira, { greeting: "*The book is upside down.*", has_lorebook: true, alternate_greetings: 2 })),
    discoverDownload: (request) => {
      downloads.push(request);
      return ok({ source: { uri: "/cache/discover/1-eira.png" }, file_name: "1-eira.png" });
    },
    importPreview: (request) => {
      expect(request.source.uri).toBe("/cache/discover/1-eira.png");
      return ok({ kind: "character_card", name: "Eira", digest: "d1", content: { kind: "character", content: { name: "Eira", description: "Witch" } }, uec3: null, images: [], warnings: [] } as never);
    },
  });
  fireEvent.click(await screen.findByText("Eira the Witch"));
  const detail = within(await screen.findByRole("dialog"));
  expect(await detail.findByText("*The book is upside down.*")).toBeTruthy();
  expect(detail.getByText("Lorebook")).toBeTruthy();
  expect(detail.getByText("+2 greetings")).toBeTruthy();
  fireEvent.click(detail.getByRole("button", { name: "Import character" }));
  await waitFor(() => expect(downloads).toEqual([{ source: "chub", kind: "character", id: "crystal/eira" }]));
  expect(await screen.findByText("Witch")).toBeTruthy();
});

it("lists card sources with capabilities and a credit line", async () => {
  await open({ discoverSearch: () => ok({ items: [], next_cursor: null, total: 0 }) });
  expect(await screen.findByText("Nothing found")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Card sources" }));
  const sheet = within(await screen.findByRole("dialog"));
  expect(sheet.getByText("character-tavern.com")).toBeTruthy();
  expect(sheet.getByText("Content from https://botbooru.com")).toBeTruthy();
  expect(sheet.getAllByText("Import").length).toBeGreaterThanOrEqual(5);
  expect(sheet.queryByText("Pygmalion")).toBeNull();
  expect(sheet.queryByText("JannyAI")).toBeNull();
});

it("browses presets as resources from Illarin", async () => {
  const requests: DiscoverSearchRequest[] = [];
  await open({
    discoverSearch: (request) => {
      requests.push(request);
      return ok({ items: [fixtureCard("p1", "AyaPrism", { source: "illarin", kind: "preset" })], next_cursor: null, total: 1 });
    },
  });
  fireEvent.click(await screen.findByRole("radio", { name: "Resources" }).catch(() => screen.getByText("Resources")));
  fireEvent.click(await screen.findByText("Presets"));
  await waitFor(() => expect(requests.at(-1)).toMatchObject({ source: "illarin", kind: "preset" }));
  expect(await screen.findByText("AyaPrism")).toBeTruthy();
});

it("keeps a long description fully in the sheet body instead of a nested scroll box", async () => {
  const description = `${"A line of the tale.\n".repeat(40)}The ending that was cut off.`;
  await open({
    discoverSearch: () => ok({ items: [eira], next_cursor: null, total: 1 }),
    discoverDetail: () => ok(fixtureDetail(eira, { description, greeting: "Hello there." })),
  });
  fireEvent.click(await screen.findByText("Eira the Witch"));
  const detail = within(await screen.findByRole("dialog"));
  const about = await detail.findByText(/The ending that was cut off/);
  expect(about.textContent).toContain("A line of the tale.");
  expect(about.className).not.toMatch(/overflow-y-auto/);
  expect(about.className).not.toMatch(/max-h-72/);
});

