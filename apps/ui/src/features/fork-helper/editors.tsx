/** Fork-only: small list editors for the helper's state and notebook. */
import { useTranslation } from "react-i18next";
import { Plus, Trash2 } from "lucide-react";
import { Button, IconButton, Input, Switch, Text, Textarea, icon } from "@lettuceai/crisp";
import type { HelperEntityDto, HelperFieldDto, HelperHolderDto, HelperTruthDto } from "@/api/generated/bindings";

const replace = <T,>(list: readonly T[], index: number, next: T | null): T[] =>
  next === null ? list.filter((_, i) => i !== index) : list.map((item, i) => (i === index ? next : item));

export function FieldsEditor({ fields, onChange }: { fields: HelperFieldDto[]; onChange: (next: HelperFieldDto[]) => void }) {
  const { t } = useTranslation("helper");
  return (
    <div className="space-y-2">
      {fields.map((field, index) => (
        <div key={index} className="flex items-center gap-2">
          <Input aria-label={t("field")} className="w-2/5" value={field.name} placeholder={t("field")} onChange={(e) => onChange(replace(fields, index, { ...field, name: e.target.value }))} />
          <Input aria-label={t("value")} className="flex-1" value={field.value} placeholder={t("value")} onChange={(e) => onChange(replace(fields, index, { ...field, value: e.target.value }))} />
          <IconButton label={t("delete")} variant="ghost" size="sm" onClick={() => onChange(replace(fields, index, null))}>
            <Trash2 size={icon.sm} />
          </IconButton>
        </div>
      ))}
      <Button size="sm" variant="ghost" leading={<Plus size={icon.sm} />} onClick={() => onChange([...fields, { name: "", value: "" }])}>
        {t("addField")}
      </Button>
    </div>
  );
}

export function EntitiesEditor({ label, entities, onChange }: { label: string; entities: HelperEntityDto[]; onChange: (next: HelperEntityDto[]) => void }) {
  const { t } = useTranslation("helper");
  return (
    <section className="space-y-2">
      <Text variant="label" className="px-1">{label}</Text>
      {entities.map((entity, index) => (
        <div key={index} className="space-y-2 rounded-xl border border-line bg-surface-1 p-3">
          <div className="flex items-center gap-2">
            <Input aria-label={t("name")} className="font-medium" value={entity.name} placeholder={t("name")} onChange={(e) => onChange(replace(entities, index, { ...entity, name: e.target.value }))} />
            <IconButton label={t("delete")} variant="ghost" size="sm" onClick={() => onChange(replace(entities, index, null))}>
              <Trash2 size={icon.sm} />
            </IconButton>
          </div>
          <FieldsEditor fields={entity.fields} onChange={(fields) => onChange(replace(entities, index, { ...entity, fields }))} />
        </div>
      ))}
      <Button size="sm" variant="ghost" leading={<Plus size={icon.sm} />} onClick={() => onChange([...entities, { name: "", fields: [{ name: "", value: "" }] }])}>
        {t("add")}
      </Button>
    </section>
  );
}

export function InventoryEditor({ holders, onChange }: { holders: HelperHolderDto[]; onChange: (next: HelperHolderDto[]) => void }) {
  const { t } = useTranslation("helper");
  return (
    <section className="space-y-2">
      <Text variant="label" className="px-1">{t("inventory")}</Text>
      {holders.map((holder, index) => (
        <div key={index} className="space-y-2 rounded-xl border border-line bg-surface-1 p-3">
          <div className="flex items-center gap-2">
            <Input aria-label={t("owner")} value={holder.owner} placeholder={t("owner")} onChange={(e) => onChange(replace(holders, index, { ...holder, owner: e.target.value }))} />
            <IconButton label={t("delete")} variant="ghost" size="sm" onClick={() => onChange(replace(holders, index, null))}>
              <Trash2 size={icon.sm} />
            </IconButton>
          </div>
          <Textarea
            aria-label={t("items")}
            autoGrow
            maxRows={4}
            value={holder.items.join(", ")}
            placeholder={t("items")}
            onChange={(e) => onChange(replace(holders, index, { ...holder, items: e.target.value.split(",").map((item) => item.trimStart()) }))}
          />
        </div>
      ))}
      <Button size="sm" variant="ghost" leading={<Plus size={icon.sm} />} onClick={() => onChange([...holders, { owner: "", items: [] }])}>
        {t("add")}
      </Button>
    </section>
  );
}

export function NotebookEditor({ truths, onChange }: { truths: HelperTruthDto[]; onChange: (next: HelperTruthDto[]) => void }) {
  const { t } = useTranslation("helper");
  const nextId = () => {
    const used = new Set(truths.map((truth) => truth.id));
    let n = 1;
    while (used.has(`T${n}`)) n++;
    return `T${n}`;
  };
  return (
    <div className="space-y-3">
      {truths.map((truth, index) => (
        <div key={index} className="space-y-2 rounded-xl border border-line bg-surface-1 p-3" data-testid="helper-truth">
          <div className="flex items-center gap-2">
            <Input aria-label={t("truthId")} className="w-16 font-mono" value={truth.id} onChange={(e) => onChange(replace(truths, index, { ...truth, id: e.target.value }))} />
            <Input aria-label={t("hiddenFrom")} className="flex-1" value={truth.hidden_from} placeholder={t("hiddenFrom")} onChange={(e) => onChange(replace(truths, index, { ...truth, hidden_from: e.target.value }))} />
            <IconButton label={t("delete")} variant="ghost" size="sm" onClick={() => onChange(replace(truths, index, null))}>
              <Trash2 size={icon.sm} />
            </IconButton>
          </div>
          <Textarea aria-label={t("truth")} autoGrow maxRows={6} value={truth.truth} placeholder={t("truth")} onChange={(e) => onChange(replace(truths, index, { ...truth, truth: e.target.value }))} />
          <label className="flex items-center justify-between gap-2 text-sm">
            <span>{t("revealed")}</span>
            <Switch checked={truth.revealed} aria-label={t("revealed")} onChange={(revealed) => onChange(replace(truths, index, { ...truth, revealed }))} />
          </label>
          <div className="space-y-1">
            <Text variant="label">{t("clues")}</Text>
            {truth.clues.length === 0 && <Text variant="hint">{t("noClues")}</Text>}
            <ul className="space-y-1 text-sm">
              {truth.clues.map((clue, clueIndex) => (
                <li key={clueIndex} className="flex gap-2">
                  <span aria-hidden>{clue.found === false ? "⚠" : "•"}</span>
                  <span className="flex-1">
                    {clue.text}
                    {clue.found !== null && <span className="ml-1 text-fg-3">{clue.found ? t("clueFound") : t("clueMissing")}</span>}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        </div>
      ))}
      <Button size="sm" variant="ghost" leading={<Plus size={icon.sm} />} onClick={() => onChange([...truths, { id: nextId(), truth: "", hidden_from: "", revealed: false, clues: [] }])}>
        {t("addTruth")}
      </Button>
    </div>
  );
}
