// Fork: hub failures, explained per error code with the backend text as details.
import { Alert, Button } from "@lettuceai/crisp";
import { useTranslation } from "react-i18next";
import { discoverErrorKey } from "./format";

export function DiscoverError({ error, site, onRetry }: { error: unknown; site: string; onRetry?: () => void }) {
  const { t } = useTranslation("home");
  return (
    <Alert
      tone="warning"
      title={t(discoverErrorKey(error), { site })}
      {...(error instanceof Error && error.message ? { details: error.message } : {})}
      {...(onRetry ? { action: <Button size="sm" variant="secondary" onClick={onRetry}>{t("discover.retry")}</Button> } : {})}
    />
  );
}
