import { describe, expect, it } from "vitest";
import type { HelperChangeDto } from "@/api/generated/bindings";
import { escapeHtml, plaqueHtml, type PlaqueLabels } from "./plaque-html";

const labels: PlaqueLabels = {
  title: "State",
  noChanges: "no changes",
  trackers: { scene: "Scene", characters: "Characters", inventory: "Inventory", notebook: "Notebook" },
  removed: "removed",
  hiddenFrom: "hidden from",
  clue: "clue",
  clueMissing: "not in the reply",
  revealed: "revealed",
  hidden: "hidden again",
  truthRemoved: "dropped",
};

const change = (patch: Partial<HelperChangeDto>): HelperChangeDto => ({
  tracker: "characters",
  kind: "set",
  subject: "",
  field: "",
  value: "",
  found: null,
  ...patch,
});

describe("plaqueHtml", () => {
  it("groups changes by tracker in a fixed order", () => {
    const html = plaqueHtml(
      [
        change({ tracker: "inventory", kind: "item_add", subject: "Аква", value: "посох" }),
        change({ subject: "Аква", field: "Одежда", value: "синее платье" }),
        change({ tracker: "scene", field: "Место", value: "таверна" }),
      ],
      labels,
    );
    expect(html.indexOf("Scene")).toBeLessThan(html.indexOf("Characters"));
    expect(html.indexOf("Characters")).toBeLessThan(html.indexOf("Inventory"));
    expect(html).toContain("<b>Аква</b> · Одежда: синее платье");
    expect(html).toContain("<summary>State · 3</summary>");
    expect(html).not.toContain("\n");
  });

  it("escapes model text", () => {
    const html = plaqueHtml([change({ subject: "<img src=x onerror=alert(1)>", field: "a", value: "<script>" })], labels);
    expect(html).not.toContain("<img");
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
  });

  it("marks removals, clues missing from the reply and reveals", () => {
    const html = plaqueHtml(
      [
        change({ field: "Одежда", subject: "Аква" }),
        change({ tracker: "notebook", kind: "clue", subject: "T1", value: "тень", found: false }),
        change({ tracker: "notebook", kind: "reveal", subject: "T2", value: "1" }),
      ],
      labels,
    );
    expect(html).toContain("Одежда: <s>removed</s>");
    expect(html).toContain("⚠ not in the reply");
    expect(html).toContain("<b>T2</b> — revealed");
  });

  it("shows an empty read as no changes", () => {
    expect(plaqueHtml([], labels)).toBe('<details class="helper-plaque"><summary>State · no changes</summary></details>');
  });

  it("escapes quotes", () => {
    expect(escapeHtml(`"a'&`)).toBe("&quot;a&#39;&amp;");
  });
});
