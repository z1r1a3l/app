/** Fork-only: recall folded chapter summaries in the prompt. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Badge, ListRow, Switch } from "@lettuceai/crisp";
import type { ChapterRecallConfigDto } from "@/api/generated/bindings";
import { useApi } from "@/api/use-api";
import { CommitNumber } from "./CommitNumber";

const KEY = ["fork-chapter-recall"] as const;
const FALLBACK: ChapterRecallConfigDto = { enabled: false, limit: 2 };

export function ChapterRecallRows() {
  const { t } = useTranslation("settings");
  const api = useApi();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: KEY,
    queryFn: () => api.call("chapterRecallConfigGet"),
  });
  const write = useMutation({
    mutationFn: (next: ChapterRecallConfigDto) => api.call("chapterRecallConfigSet", next),
    onSuccess: (view) => client.setQueryData(KEY, view),
  });
  const value = query.data ?? FALLBACK;
  const set = (patch: Partial<ChapterRecallConfigDto>) => write.mutate({ ...value, ...patch });
  return (
    <>
      <ListRow
        title={
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
            {t("memory.advanced.chapterRecall")} <Badge>{t("memory.advanced.experimental")}</Badge>
          </span>
        }
        description={t("memory.advanced.chapterRecallBody")}
        trailing={<Switch checked={value.enabled} onChange={(next) => set({ enabled: next })} aria-label={t("memory.advanced.chapterRecall")} />}
      />
      {value.enabled && (
        <ListRow
          title={t("memory.advanced.chapterRecallLimit")}
          description={t("memory.advanced.chapterRecallLimitBody")}
          trailing={<CommitNumber value={value.limit} min={1} max={2} step={1} onCommit={(next) => set({ limit: next })} />}
        />
      )}
    </>
  );
}
