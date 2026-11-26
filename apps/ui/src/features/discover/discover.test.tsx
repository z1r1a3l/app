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
  expect(requests[0]).toMatchObject({ source: "chub", kind: "character", nsfw: true, sort: "default", cursor: null });

  fireEvent.click(screen.getByRole("tab", { name: "Wyvern" }));
  await waitFor(() => expect(requests.at(-1)).toMatchObject({ source: "wyvern", sort: "total_likes" }));
  await waitFor(() => expect(screen.queryByText("Eira the Witch")).toBeNull());

  fireEvent.click(screen.getAllByRole("switch", { name: "Show adult content" })[0]!);
  await waitFor(() => expect(requests.at(-1)).toMatchObject({ source: "wyvern", nsfw: false }));
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
  expect(sheet.getAllByText("Import").length).toBeGreaterThan(5);
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
