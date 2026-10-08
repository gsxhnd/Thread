import { useEffect, useState } from "react";
import { toast } from "sonner";
import { SelectField } from "@/components/ui/select-field";
import { environmentActivate, sessionGet } from "@/lib/api";
import type { RecentWorkspace } from "@/lib/api";
import { activeEnvironmentPath, useAppStore } from "@/stores/app-store";
import { useTabsStore } from "@/stores/tabs-store";
import { useUiStore } from "@/stores/ui-store";

/**
 * 工作区切换（标题栏左侧）。
 *
 * 列出当前工作区与最近打开过的工作区；切换后关闭所有标签页并回到请求视图，
 * 避免旧工作区的请求路径残留。
 */
export function WorkspaceSwitcher({ className }: { className?: string }) {
  const snapshot = useAppStore((state) => state.snapshot);
  const openWorkspace = useAppStore((state) => state.openWorkspace);
  const [recent, setRecent] = useState<RecentWorkspace[]>([]);

  const root = snapshot?.index.root;

  useEffect(() => {
    if (!root) return;
    let cancelled = false;
    void sessionGet()
      .then((session) => {
        if (!cancelled) setRecent(session.recentWorkspaces);
      })
      .catch(() => {
        if (!cancelled) setRecent([]);
      });
    return () => {
      cancelled = true;
    };
  }, [root]);

  if (!snapshot || !root) return null;

  const options = [
    { path: root, name: snapshot.index.name },
    ...recent.filter((item) => item.path !== root),
  ];

  const handleChange = async (path: string) => {
    if (!path || path === root) return;
    const item = recent.find((entry) => entry.path === path);
    try {
      await openWorkspace(path, item?.name, "open");
      useTabsStore.getState().closeAll();
      useUiStore.getState().setView("request");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <SelectField
      aria-label="工作区"
      title={`切换工作区（当前：${root}）`}
      className={className}
      value={root}
      onValueChange={(path) => void handleChange(path)}
      options={options.map((item) => ({ value: item.path, label: item.name }))}
    />
  );
}

/** 环境切换（标题栏左侧）：激活结果写入工作区配置。 */
export function EnvironmentSwitcher({ className }: { className?: string }) {
  const snapshot = useAppStore((state) => state.snapshot);
  const setSnapshot = useAppStore((state) => state.setSnapshot);

  if (!snapshot) return null;

  const environmentPath = activeEnvironmentPath(snapshot);
  const environments = snapshot.index.environments;

  const handleChange = async (path: string) => {
    try {
      const next = await environmentActivate(path);
      setSnapshot(next);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <SelectField
      aria-label="环境"
      title="切换环境（变量来自 environments/*.env.json）"
      className={className}
      value={environmentPath ?? ""}
      onValueChange={(path) => void handleChange(path)}
      options={[
        { value: "", label: "未激活环境" },
        ...environments.map((environment) => ({
          value: environment.path,
          label: environment.name,
        })),
      ]}
    />
  );
}
