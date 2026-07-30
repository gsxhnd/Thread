import { useQuery } from "@tanstack/react-query";
import { Link, Outlet, useParams } from "@tanstack/react-router";
import {
  BookMarked,
  Clock3,
  FileText,
  GitBranch,
  Home,
  PanelLeft,
  PanelLeftClose,
  Settings,
  Sparkles,
  Users,
  Globe2,
  BarChart3,
} from "lucide-react";
import { useEffect } from "react";
import { WindowTitleBar } from "@/components/window-title-bar";
import { Button } from "@/components/ui/button";
import { projectGet } from "@/lib/api";
import { getPlatform } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useEditorStore } from "@/stores/editor-store";
import { useUiStore } from "@/stores/ui-store";

const NAV = [
  { to: "/project/$projectId/write", label: "写作", icon: FileText },
  { to: "/project/$projectId/outline", label: "大纲", icon: GitBranch },
  { to: "/project/$projectId/characters", label: "人物", icon: Users },
  { to: "/project/$projectId/world", label: "世界观", icon: Globe2 },
  { to: "/project/$projectId/timeline", label: "时间线", icon: Clock3 },
  { to: "/project/$projectId/foreshadowing", label: "伏笔", icon: Sparkles },
  { to: "/project/$projectId/stats", label: "统计", icon: BarChart3 },
  { to: "/project/$projectId/settings", label: "设置", icon: Settings },
] as const;

export function ProjectLayout() {
  const { projectId } = useParams({ from: "/project/$projectId" });
  const { navCollapsed, setNavCollapsed } = useUiStore();
  const platform = getPlatform();

  const { data: project } = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => projectGet(projectId),
  });

  useEffect(() => {
    return () => {
      useEditorStore.getState().clear();
    };
  }, [projectId]);

  return (
    <div
      data-app-shell
      data-platform={platform}
      className="flex h-screen flex-col bg-shell-main text-foreground"
    >
      <WindowTitleBar
        trailing={
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label={navCollapsed ? "展开侧边栏" : "折叠侧边栏"}
            onClick={() => setNavCollapsed(!navCollapsed)}
          >
            {navCollapsed ? (
              <PanelLeft className="size-3.5" />
            ) : (
              <PanelLeftClose className="size-3.5" />
            )}
          </Button>
        }
      >
        <BookMarked className="size-4 shrink-0 text-primary" />
        <span className="truncate text-sm font-semibold">
          {project?.title ?? "Thread"}
        </span>
      </WindowTitleBar>

      <div className="flex min-h-0 flex-1">
        <aside
          className={cn(
            "flex shrink-0 flex-col border-r border-border bg-shell-primary transition-[width] duration-200 ease-linear",
            navCollapsed ? "w-12" : "w-52",
          )}
        >
          <Link
            to="/"
            title="返回书架"
            className={cn(
              "mx-2 mt-2 mb-2 flex items-center gap-2 rounded-md px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground",
              navCollapsed && "justify-center px-0",
            )}
          >
            <Home className="size-3.5 shrink-0" />
            {!navCollapsed && <span>返回书架</span>}
          </Link>
          <nav className="flex flex-1 flex-col gap-0.5 px-2 pb-2">
            {NAV.map((item) => (
              <Link
                key={item.to}
                to={item.to}
                params={{ projectId }}
                title={item.label}
                className={cn(
                  "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition hover:bg-accent hover:text-accent-foreground",
                  navCollapsed && "justify-center px-0",
                )}
                activeProps={{
                  className: cn(
                    "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm bg-accent text-accent-foreground font-medium",
                    navCollapsed && "justify-center px-0",
                  ),
                }}
              >
                <item.icon className="size-4 shrink-0" />
                {!navCollapsed && <span>{item.label}</span>}
              </Link>
            ))}
          </nav>
        </aside>
        <div className="min-w-0 flex-1">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
