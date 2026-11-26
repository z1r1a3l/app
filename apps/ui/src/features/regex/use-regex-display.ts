/** Fork-only: a message's text after display regex scripts. */
import { useContext, useMemo } from "react";
import { RegexDisplayContext } from "./display-context";
import { applyRegexScripts, placement } from "./engine";
import { hasRichMarkup } from "./rich-html";

export interface DisplayText {
  text: string;
  rich: boolean;
}

/**
 * The text a message shows after display regex; `messageId` null means the
 * reply streaming now (depth 0).
 */
export function useRegexDisplay(text: string, role: string, messageId: string | null): DisplayText {
  const display = useContext(RegexDisplayContext);
  return useMemo(() => {
    if (!display || (role !== "assistant" && role !== "user")) return { text, rich: false };
    const depth = messageId === null ? 0 : display.depthOf(messageId);
    const out = applyRegexScripts(
      display.scripts,
      text,
      {
        placement: role === "user" ? placement.userInput : placement.aiOutput,
        target: "display",
        ...(depth !== undefined ? { depth } : {}),
      },
      display.macros,
    );
    return { text: out, rich: out !== text && hasRichMarkup(out) };
  }, [display, text, role, messageId]);
}
