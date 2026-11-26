/** Fork-only: the `{{roll}}` results of a reply's prompt, for its details sheet. */
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Dices } from "lucide-react";
import { icon } from "@lettuceai/crisp";
import type { ApiClient } from "@/api/client";
import { macroQueryKeys } from "./queries";
import { rollLine } from "./rolls";

export function MessageDice({ api, messageId }: { api: ApiClient; messageId: string }) {
  const { t } = useTranslation("macros");
  const rolls = useQuery({
    queryKey: macroQueryKeys.rolls(messageId),
    queryFn: () => api.call("macroMessageRolls", { message_id: messageId }),
    staleTime: Infinity,
  });
  const list = rolls.data?.rolls ?? [];
  if (list.length === 0) return null;
  return (
    <section className="space-y-2" data-testid="message-dice">
      <div className="flex items-center gap-2 text-sm font-medium text-fg">
        <Dices size={icon.md} className="text-fg-3" aria-hidden />
        {t("dice.title")}
      </div>
      <p className="text-xs text-fg-3">{t("dice.hint")}</p>
      <ul className="space-y-1">
        {list.map((roll, index) => (
          <li key={index} className="flex items-baseline justify-between gap-3 rounded-lg bg-fill px-3 py-1.5 text-sm">
            <span className="min-w-0 truncate font-mono text-fg-2">{rollLine(roll)}</span>
            <span className="shrink-0 font-semibold tabular-nums text-fg">{roll.total}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
