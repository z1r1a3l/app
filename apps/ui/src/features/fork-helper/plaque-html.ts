/**
 * Fork-only: the collapsible plaque under a reply, as HTML for the safe rich
 * path (sanitized and rendered in a Shadow DOM). Values come from the model,
 * so every one is escaped here too.
 */
import type { HelperChangeDto } from "@/api/generated/bindings";

export interface PlaqueLabels {
  title: string;
  noChanges: string;
  trackers: Record<string, string>;
  removed: string;
  hiddenFrom: string;
  clue: string;
  clueMissing: string;
  revealed: string;
  hidden: string;
  truthRemoved: string;
}

/** The author's notebook stays out of the plaque: it is spoilers, shown only on its own tab. */
const ORDER = ["scene", "characters", "bonds", "inventory", "npcs"];

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c);
}

function line(change: HelperChangeDto, labels: PlaqueLabels): string {
  const subject = `<b>${escapeHtml(change.subject)}</b>`;
  const value = escapeHtml(change.value);
  const field = escapeHtml(change.field);
  switch (change.kind) {
    case "set": {
      const shown = change.value ? value : `<s>${escapeHtml(labels.removed)}</s>`;
      return change.subject ? `${subject} · ${field}: ${shown}` : `${field}: ${shown}`;
    }
    case "remove":
      return `${subject} — <s>${escapeHtml(labels.removed)}</s>`;
    case "item_add":
      return `${subject} + ${value}`;
    case "item_remove":
      return `${subject} − <s>${value}</s>`;
    case "truth":
      return `${subject}: ${value}${change.field ? ` <i>(${escapeHtml(labels.hiddenFrom)}: ${field})</i>` : ""}`;
    case "clue":
      return `${subject} · ${escapeHtml(labels.clue)}: ${value}${change.found === false ? ` <i>⚠ ${escapeHtml(labels.clueMissing)}</i>` : ""}`;
    case "reveal":
      return `${subject} — ${escapeHtml(change.value === "0" ? labels.hidden : labels.revealed)}`;
    case "truth_remove":
      return `${subject} — <s>${escapeHtml(labels.truthRemoved)}</s>`;
    default:
      return `${subject} ${value}`;
  }
}

/** One line of HTML (no blank lines, so markdown keeps it as one block). */
export function plaqueHtml(all: readonly HelperChangeDto[], labels: PlaqueLabels): string {
  const changes = all.filter((change) => ORDER.includes(change.tracker));
  const summary = `<summary>${escapeHtml(labels.title)} · ${changes.length || escapeHtml(labels.noChanges)}</summary>`;
  if (changes.length === 0) return `<details class="helper-plaque">${summary}</details>`;
  const sections = ORDER.flatMap((tracker) => {
    const own = changes.filter((change) => change.tracker === tracker);
    if (own.length === 0) return [];
    const items = own.map((change) => `<br>• ${line(change, labels)}`).join("");
    return [`<p><b>${escapeHtml(labels.trackers[tracker] ?? tracker)}</b>${items}</p>`];
  });
  return `<details class="helper-plaque">${summary}${sections.join("")}</details>`;
}
