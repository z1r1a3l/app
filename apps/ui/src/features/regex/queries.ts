export const regexQueryKeys = {
  all: ["fork-regex"] as const,
  scope: (kind: string, id: string | null) => ["fork-regex", "scope", kind, id ?? ""] as const,
  conversation: (conversationId: string) => ["fork-regex", "conversation", conversationId] as const,
  pending: () => ["fork-regex", "pending"] as const,
};
