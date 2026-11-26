/** Fork-only: Settings → Regex scripts, laid out like the other settings pages. */
import { RegexPage } from "@/features/regex";
import { useSettingsLayout } from "./data";

export function RegexSection() {
  const { phone, back } = useSettingsLayout();
  return <RegexPage layout={{ phone, back }} />;
}
