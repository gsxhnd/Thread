import i18n from "@/i18n";
import { Button } from "@/components/ui/button";

type AppFatalScreenProps = {
  title: string;
  description: string;
  detail?: string;
  onRetry?: () => void;
};

/** Full-window fatal error UI with reload and optional retry. */
export function AppFatalScreen({
  title,
  description,
  detail,
  onRetry,
}: AppFatalScreenProps) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-6 text-foreground">
      <div className="w-full max-w-lg space-y-4 rounded-xl border border-border bg-card p-6 shadow-sm">
        <div className="space-y-2">
          <h1 className="text-lg font-semibold">{title}</h1>
          <p className="text-sm text-muted-foreground">{description}</p>
          {detail ? (
            <pre className="max-h-40 overflow-auto rounded-md bg-muted/60 p-3 text-xs whitespace-pre-wrap break-words text-muted-foreground">
              {detail}
            </pre>
          ) : null}
        </div>
        <div className="flex flex-wrap gap-2">
          {onRetry ? (
            <Button type="button" variant="outline" onClick={onRetry}>
              {i18n.t("common:errorBoundary.retry")}
            </Button>
          ) : null}
          <Button type="button" onClick={() => window.location.reload()}>
            {i18n.t("common:errorBoundary.reload")}
          </Button>
        </div>
      </div>
    </div>
  );
}
