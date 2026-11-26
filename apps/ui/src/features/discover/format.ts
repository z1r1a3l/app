// Fork: small Discover helpers kept out of component files.
import { compactNumber } from "@lettuceai/crisp";
import { isApiFailure } from "@/api/client";
import type { DiscoverCard } from "@/api/generated/bindings";

const KNOWN = ["busy", "unavailable", "not_found", "unsupported", "malformed", "transport"] as const;
type Known = (typeof KNOWN)[number];

export function discoverErrorKey(error: unknown): `discover.error.${Known | "other"}` {
  const code = isApiFailure(error) ? error.code : "internal";
  const known = KNOWN.find((item) => item === code);
  return `discover.error.${known ?? "other"}`;
}

/** The most telling popularity number the hub gave. */
export function headlineStat(card: DiscoverCard): string | null {
  const { stats } = card;
  const value = stats.downloads ?? stats.stars ?? stats.likes ?? stats.chats ?? stats.views ?? stats.messages;
  return value == null ? null : compactNumber(value);
}
