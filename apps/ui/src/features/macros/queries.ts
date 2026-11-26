export const macroQueryKeys = {
  all: ["fork-macros"] as const,
  vars: (conversationId: string | null) => ["fork-macros", "vars", conversationId ?? ""] as const,
  rolls: (messageId: string) => ["fork-macros", "rolls", messageId] as const,
  preview: (texts: readonly string[], conversationId: string | null, user: string | null, char: string | null) =>
    ["fork-macros", "preview", conversationId ?? "", user ?? "", char ?? "", texts] as const,
};
