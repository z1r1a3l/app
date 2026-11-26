export const helperQueryKeys = {
  all: ["fork-helper"] as const,
  config: () => ["fork-helper", "config"] as const,
  chat: (conversationId: string) => ["fork-helper", "chat", conversationId] as const,
  status: (conversationId: string) => ["fork-helper", "status", conversationId] as const,
  message: (conversationId: string, messageId: string, variant: string) =>
    ["fork-helper", "message", conversationId, messageId, variant] as const,
};
