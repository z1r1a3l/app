import type { DiceRollDto } from "@/api/generated/bindings";

/** "2d6+3: 4 + 2 + 3 = 9", "1d20: 17". */
export function rollLine(roll: DiceRollDto): string {
  const parts = roll.dice.map(String);
  if (roll.modifier !== 0) parts.push(roll.modifier > 0 ? String(roll.modifier) : `(${roll.modifier})`);
  const sum = parts.length > 1 ? `${parts.join(" + ")} = ${roll.total}` : String(roll.total);
  return `${roll.formula}: ${sum}`;
}
