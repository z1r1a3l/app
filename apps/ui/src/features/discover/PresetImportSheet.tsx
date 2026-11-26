// Fork: confirm a downloaded prompt preset; the regular import sheet only
// covers characters, personas and lorebooks.
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { FileText } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button, icon, ListGroup, ListRow, Sheet, toast } from "@lettuceai/crisp";
import type { ApiClient } from "@/api/client";
import { queryKeys } from "@/api/query-keys";
import type { FileSource, ImportPreview_Serialize } from "@/api/generated/bindings";
import { commitImport, refreshLibrary } from "@/features/library";
import { FailureState } from "@/shared/ui/FailureState";

export function PresetImportSheet({
  api,
  source,
  preview,
  onClose,
}: {
  api: ApiClient;
  source: FileSource;
  preview: ImportPreview_Serialize;
  onClose: () => void;
}) {
  const { t } = useTranslation("home");
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [operationId] = useState(() => crypto.randomUUID());
  const content = preview.content.kind === "prompt" ? preview.content : null;
  const commit = useMutation({
    mutationFn: () => commitImport(api, source, preview.digest, operationId),
    onSuccess: (result) => {
      onClose();
      void refreshLibrary(queryClient);
      void queryClient.invalidateQueries({ queryKey: queryKeys.prompts.all });
      if (result.kind !== "imported") return;
      toast.success(t("discover.presetImported", { name: preview.name }), undefined, {
        action: {
          label: t("discover.open"),
          onClick: () => void navigate({ to: "/settings/$section", params: { section: "prompts" }, search: { prompt: result.id } }),
        },
      });
    },
  });
  return (
    <Sheet
      open
      onClose={() => !commit.isPending && onClose()}
      size="sm"
      title={t("discover.importPresetTitle")}
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" className="flex-1" disabled={commit.isPending} onClick={onClose}>
            {t("discover.cancel")}
          </Button>
          <Button variant="primary" className="flex-1" loading={commit.isPending} onClick={() => commit.mutate()}>
            {t("discover.import.preset")}
          </Button>
        </div>
      }
    >
      <div className="space-y-4 pb-1">
        <ListGroup>
          <ListRow
            icon={<FileText size={icon.lg} />}
            title={preview.name}
            description={content ? t("discover.presetEntries", { count: content.entries.length }) : undefined}
          />
        </ListGroup>
        {preview.warnings.length > 0 && <p className="px-1 text-xs text-fg-3">{t("discover.presetNotes", { count: preview.warnings.length })}</p>}
        {commit.isError && <FailureState error={commit.error} size="sm" />}
      </div>
    </Sheet>
  );
}
