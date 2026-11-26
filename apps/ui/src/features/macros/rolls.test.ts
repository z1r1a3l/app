import { describe, expect, it } from "vitest";
import { rollLine } from "./rolls";
import { varRecord } from "./data";

describe("fork macros", () => {
  it("writes a roll as its dice and total", () => {
    expect(rollLine({ formula: "1d20", dice: [17], modifier: 0, total: 17 })).toBe("1d20: 17");
    expect(rollLine({ formula: "2d6+3", dice: [4, 2], modifier: 3, total: 9 })).toBe("2d6+3: 4 + 2 + 3 = 9");
    expect(rollLine({ formula: "d8-1", dice: [5], modifier: -1, total: 4 })).toBe("d8-1: 5 + (-1) = 4");
  });

  it("turns a variable list into a record", () => {
    expect(varRecord([{ name: "a", value: "1" }])).toEqual({ a: "1" });
    expect(varRecord(undefined)).toEqual({});
  });
});
