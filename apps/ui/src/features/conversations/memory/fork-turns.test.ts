import { describe, expect, it } from "vitest";
import { inputsOf, parseTurnInputs, rangeOfLabel, sameRange } from "./fork-turns";

describe("parseTurnInputs", () => {
  it("treats empty fields as no turns", () => {
    expect(parseTurnInputs("", "")).toEqual({ range: null });
    expect(parseTurnInputs("  ", " ")).toEqual({ range: null });
  });

  it("accepts a range and a single filled field", () => {
    expect(parseTurnInputs("10", "20")).toEqual({ range: { from: 10, to: 20 } });
    expect(parseTurnInputs("7", "7")).toEqual({ range: { from: 7, to: 7 } });
    expect(parseTurnInputs("12", "")).toEqual({ range: { from: 12, to: 12 } });
    expect(parseTurnInputs("", "12")).toEqual({ range: { from: 12, to: 12 } });
  });

  it("rejects zero, reversed, and non-integer values", () => {
    expect(parseTurnInputs("0", "10")).toEqual({ error: "invalid" });
    expect(parseTurnInputs("20", "10")).toEqual({ error: "invalid" });
    expect(parseTurnInputs("1.5", "2")).toEqual({ error: "invalid" });
    expect(parseTurnInputs("10", "x")).toEqual({ error: "invalid" });
    expect(parseTurnInputs("-3", "4")).toEqual({ error: "invalid" });
  });
});

describe("range helpers", () => {
  it("reads a cycle label and round-trips inputs", () => {
    expect(rangeOfLabel("10-20")).toEqual({ from: 10, to: 20 });
    expect(rangeOfLabel(null)).toBeNull();
    expect(inputsOf({ from: 10, to: 20 })).toEqual({ from: "10", to: "20" });
    expect(sameRange({ from: 10, to: 20 }, { from: 10, to: 20 })).toBe(true);
    expect(sameRange({ from: 10, to: 20 }, { from: 10, to: 21 })).toBe(false);
    expect(sameRange(null, null)).toBe(true);
  });
});
