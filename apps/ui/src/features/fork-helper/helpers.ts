/** Fork-only: helper model UI helpers. */
import type { HelperChatView, TimelineMessage } from "@/api/generated/bindings";
import type { ModelRequirement } from "@/entities/model";

const LOCAL = new Set(["ollama", "llama_cpp", "stable_diffusion"]);
/** Cloud chat models only: the helper runs next to the main model. */
export const helperModelRequirement: ModelRequirement = {
  kind: "chat",
  input: ["text"],
  output: ["text"],
  allow: (choice) => !LOCAL.has(choice.account.protocol),
};

export const TRACKERS = ["scene", "characters", "bonds", "inventory", "npcs", "notebook"] as const;

/** Where tracker switches are written: the row in effect, else the chat. */
export function trackerTarget(view: HelperChatView): { scope: "conversation" | "character"; id: string; enabled: boolean } {
  if (view.chat) return { scope: "conversation", id: view.conversation_id, enabled: view.chat.enabled };
  if (view.character?.enabled && view.character_id) return { scope: "character", id: view.character_id, enabled: true };
  return { scope: "conversation", id: view.conversation_id, enabled: false };
}

/** Changes when the shown variant or its text changes (swipe, edit). */
export function variantOf(message: TimelineMessage): string {
  const text = JSON.stringify(message.parts);
  let hash = 0;
  for (let i = 0; i < text.length; i++) hash = (hash * 31 + text.charCodeAt(i)) | 0;
  return `${message.candidate_index ?? "-"}:${text.length}:${hash}`;
}

