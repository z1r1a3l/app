import { windowOf } from "../data/memory";

export interface TurnRange {
  from: number;
  to: number;
}

export function rangeOfLabel(label: string | null | undefined): TurnRange | null {
  if (!label) return null;
  return windowOf(label);
}

export function parseTurnInputs(from: string, to: string): { range: TurnRange | null } | { error: "invalid" } {
  const start = parseTurn(from);
  const end = parseTurn(to);
  if (start === "invalid" || end === "invalid") return { error: "invalid" };
  if (start === null && end === null) return { range: null };
  const fromTurn = start ?? end!;
  const toTurn = end ?? start!;
  if (fromTurn > toTurn) return { error: "invalid" };
  return { range: { from: fromTurn, to: toTurn } };
}

export function inputsOf(range: TurnRange | null): { from: string; to: string } {
  if (range === null) return { from: "", to: "" };
  return { from: String(range.from), to: String(range.to) };
}

export function sameRange(left: TurnRange | null, right: TurnRange | null): boolean {
  if (left === null || right === null) return left === right;
  return left.from === right.from && left.to === right.to;
}

export function toContract(range: TurnRange): { start: number; end: number } {
  return { start: range.from, end: range.to };
}

function parseTurn(value: string): number | null | "invalid" {
  const trimmed = value.trim();
  if (trimmed.length === 0) return null;
  if (!/^[1-9]\d*$/.test(trimmed)) return "invalid";
  const turn = Number(trimmed);
  if (!Number.isSafeInteger(turn) || turn < 1) return "invalid";
  return turn;
}
