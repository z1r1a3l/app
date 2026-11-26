// Fork: the list of hubs Discover reads, with what each can do and a credit line.
import { Check, Globe, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { icon, ListGroup, ListRowButton, Sheet } from "@lettuceai/crisp";
import type { DiscoverKind, DiscoverSource } from "@/api/generated/bindings";
import { type Capability, SOURCES } from "./sources";

const CAPABILITIES: readonly Capability[] = ["search", "preview", "import"];

export function CapabilityChips({ have }: { have: readonly Capability[] }) {
  const { t } = useTranslation("home");
  return (
    <span className="flex flex-wrap gap-1">
      {CAPABILITIES.map((capability) => {
        const on = have.includes(capability);
        return (
          <span
            key={capability}
            className={`inline-flex items-center gap-0.5 rounded-full px-2 py-0.5 text-[11px] ${on ? "bg-accent/15 text-accent" : "bg-fill text-fg-3 line-through"}`}
          >
            {on ? <Check size={11} /> : <X size={11} />}
            {t(`discover.capability.${capability}`)}
          </span>
        );
      })}
    </span>
  );
}

export function SourcesSheet({
  open,
  kind,
  current,
  onPick,
  onClose,
}: {
  open: boolean;
  kind: DiscoverKind;
  current: DiscoverSource;
  onPick: (source: DiscoverSource) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation("home");
  return (
    <Sheet open={open} onClose={onClose} title={t("discover.sources.title")} description={t("discover.sources.description")}>
      <div className="space-y-4 pb-2">
        <ListGroup>
          {SOURCES.map((source) => {
            const supported = source.kinds.includes(kind);
            const have = source.capabilities[kind] ?? [];
            return (
              <ListRowButton
                key={source.id}
                icon={<Globe size={icon.lg} />}
                tone={source.id === current ? "accent" : "neutral"}
                disabled={!supported}
                chevron={false}
                trailing={source.id === current ? <Check size={icon.md} /> : undefined}
                onClick={() => {
                  onPick(source.id);
                  onClose();
                }}
                title={
                  <span className="flex items-baseline gap-2">
                    <span>{source.name}</span>
                    <span className="text-xs font-normal text-fg-3">{source.domain}</span>
                  </span>
                }
                description={
                  <span className="mt-1 flex flex-col gap-1.5">
                    {supported ? <CapabilityChips have={have} /> : <span>{t("discover.sources.notForKind")}</span>}
                    {source.note && <span className="text-xs">{t(`discover.sourceNote.${source.note}`)}</span>}
                    <span className="text-[11px] text-fg-3">{t("discover.sources.credit", { url: source.url })}</span>
                  </span>
                }
              />
            );
          })}
        </ListGroup>
        <p className="px-1 text-xs text-fg-3">{t("discover.sources.footer")}</p>
      </div>
    </Sheet>
  );
}
