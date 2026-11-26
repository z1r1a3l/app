// Fork (z1r1a3l): chat header shortcut to the chat's lorebooks. The sheet lives in
// features/fork-lorebook and loads lazily so the header stays light.
import { lazy, Suspense, useState } from "react";
import { BookOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { icon, IconButton } from "@lettuceai/crisp";
import type { TimelineMessage } from "@/api/generated/bindings";

const ChatLorebooksSheet = lazy(() =>
  import("@/features/fork-lorebook").then((module) => ({ default: module.ChatLorebooksSheet })),
);

export function ForkLorebookButton({
  conversationId,
  characterId,
  messages,
  phone,
}: {
  conversationId: string;
  characterId: string | null;
  messages: readonly TimelineMessage[];
  phone: boolean;
}) {
  const { t } = useTranslation("fork-lorebook");
  const [open, setOpen] = useState(false);
  const [mounted, setMounted] = useState(false);
  return (
    <>
      <IconButton
        label={t("button")}
        onClick={() => {
          setMounted(true);
          setOpen(true);
        }}
      >
        <BookOpen size={icon.lg} />
      </IconButton>
      {mounted && (
        <Suspense fallback={null}>
          <ChatLorebooksSheet
            open={open}
            onClose={() => setOpen(false)}
            conversationId={conversationId}
            characterId={characterId}
            messages={messages}
            phone={phone}
          />
        </Suspense>
      )}
    </>
  );
}
