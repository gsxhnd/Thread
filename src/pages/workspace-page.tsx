import { useEffect } from "react";
import { useNavigate } from "@tanstack/react-router";
import { LogOutIcon, ShieldAlertIcon } from "lucide-react";
import { toast } from "sonner";
import { BrandMark } from "@/components/brand-mark";
import { Button } from "@/components/ui/button";
import { WindowTitleBar } from "@/components/window-title-bar";
import { onWorkspaceChanged } from "@/lib/api";
import { useAppStore } from "@/stores/app-store";
import { useTabsStore } from "@/stores/tabs-store";
import { useUiStore } from "@/stores/ui-store";
import { Sidebar } from "@/components/workspace/sidebar";
import { EnvironmentSwitcher, WorkspaceSwitcher } from "@/components/workspace/switchers";
import { RequestWorkspace } from "@/components/request/request-workspace";
import { HistoryView } from "@/components/history/history-view";
import { EnvironmentsView } from "@/components/environment/environments-view";
import { SettingsView } from "@/components/settings/settings-view";

/** 工作区外壳：标题栏 + 侧边栏 + 主区域（请求 / 历史 / 环境 / 设置）。 */
export function WorkspacePage() {
  const snapshot = useAppStore((state) => state.snapshot);
  const refresh = useAppStore((state) => state.refresh);
  const closeWorkspace = useAppStore((state) => state.closeWorkspace);
  const view = useUiStore((state) => state.view);
  const navigate = useNavigate();
  const warnings = snapshot?.index.warnings ?? [];

  // 外部修改 / Git 切换分支：后端轮询到变化后推送事件，这里重新拉取索引。
  useEffect(
    () =>
      onWorkspaceChanged((payload) => {
        void refresh();
        toast.info(
          `工作区文件已变化（${payload.requestCount} 个请求 / ${payload.collectionCount} 个集合），已刷新`,
        );
      }),
    [refresh],
  );

  // 关闭工作区后回到欢迎页（路由守卫只在导航时检查）。
  useEffect(() => {
    if (!snapshot) void navigate({ to: "/" });
  }, [snapshot, navigate]);

  if (!snapshot) return null;

  const handleClose = async () => {
    useTabsStore.getState().closeAll();
    useUiStore.getState().reset();
    await closeWorkspace();
  };

  return (
    <div data-app-root className="flex h-screen flex-col bg-shell-main text-foreground">
      <WindowTitleBar
        trailing={
          <Button variant="ghost" size="icon-xs" title="关闭工作区" onClick={() => void handleClose()}>
            <LogOutIcon />
          </Button>
        }
      >
        <span className="flex size-5 items-center justify-center rounded-md bg-primary text-primary-foreground">
          <BrandMark className="size-3.5" />
        </span>
        <span className="shrink-0 text-sm font-medium">Thread</span>
        <span className="shrink-0 text-muted-foreground/70">/</span>
        {/* 标题栏左侧的工作区 / 环境切换：需要 pointer-events-auto 才能点击（父层禁用了指针事件）。 */}
        <div
          data-no-drag
          className="pointer-events-auto flex min-w-0 items-center gap-1.5"
        >
          <WorkspaceSwitcher className="w-44" />
          <EnvironmentSwitcher className="w-32" />
        </div>
      </WindowTitleBar>

      {warnings.length > 0 ? (
        <div className="flex items-start gap-2 border-b border-notice/30 bg-notice-bg px-3 py-1.5 text-[11px] text-notice-foreground">
          <ShieldAlertIcon className="mt-0.5 size-3.5 shrink-0" />
          <span className="min-w-0 flex-1">
            {warnings.slice(0, 3).join(" · ")}
            {warnings.length > 3 ? ` …（共 ${warnings.length} 条）` : ""}
          </span>
        </div>
      ) : null}

      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          {view === "request" ? <RequestWorkspace /> : null}
          {view === "history" ? <HistoryView /> : null}
          {view === "environments" ? <EnvironmentsView /> : null}
          {view === "settings" ? <SettingsView /> : null}
        </main>
      </div>
    </div>
  );
}
