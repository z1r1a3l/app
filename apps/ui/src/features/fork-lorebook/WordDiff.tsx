// Fork (z1r1a3l): word diff that wraps, for prose entries on a phone screen.
import { useMemo } from "react";
import { diffArrays } from "diff";
import { cn } from "@lettuceai/crisp";
import { tokens } from "./data";

export function WordDiff({ before, after, className }: { before: string; after: string; className?: string }) {
  const parts = useMemo(
    () => diffArrays(tokens(before), tokens(after)).map((part) => ({ ...part, value: part.value.join("") })),
    [before, after],
  );
  return (
    <p className={cn("whitespace-pre-wrap break-words rounded-xl bg-fill p-3 text-sm leading-relaxed text-fg-2", className)}>
      {parts.map((part, index) =>
        part.added ? (
          <ins key={index} className="rounded bg-accent/20 text-fg no-underline">
            {part.value}
          </ins>
        ) : part.removed ? (
          <del key={index} className="rounded bg-danger/15 text-danger line-through">
            {part.value}
          </del>
        ) : (
          <span key={index}>{part.value}</span>
        ),
      )}
    </p>
  );
}

