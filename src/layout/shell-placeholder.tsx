import { useTranslation } from "react-i18next";

type ShellPlaceholderProps = {
  /** i18n key under the `common:shells` namespace, e.g. "main" / "settings". */
  titleKey: string;
};

/**
 * Empty-state shell used by every window in the starter template.
 *
 * 模板中每个窗口共用的空白占位界面。带上 `data-app-shell` 以复用 App.css 中的
 * shell 背景与圆角样式，方便直接在此基础上搭建真实业务界面。
 */
export function ShellPlaceholder({ titleKey }: ShellPlaceholderProps) {
  const { t } = useTranslation("common");

  return (
    <div
      data-app-shell
      className="flex h-svh flex-col items-center justify-center gap-3 bg-shell-main p-8 text-center"
    >
      <h1 className="text-2xl font-semibold tracking-tight">{t("app.name")}</h1>
      <p className="text-sm font-medium text-foreground/80">{t(`shells.${titleKey}`)}</p>
      <p className="max-w-md text-sm text-muted-foreground">{t("shells.placeholder")}</p>
    </div>
  );
}
