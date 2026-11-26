/** Fork-only: the display-regex context shared by the provider and the hook. */
import { createContext } from "react";
import type { MacroValues, RegexScript } from "./engine";

export interface RegexDisplay {
  scripts: readonly RegexScript[];
  macros: MacroValues;
  depthOf: (messageId: string) => number | undefined;
}

export const RegexDisplayContext = createContext<RegexDisplay | null>(null);
