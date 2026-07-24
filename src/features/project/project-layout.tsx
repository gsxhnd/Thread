import { Link, Outlet, useParams } from "@tanstack/react-router";
import {
  BookMarked,
  Clock3,
  FileText,
  GitBranch,
  Home,
  Settings,
  Sparkles,
  Users,
  Globe2,
  BarChart3,
} from "lucide-react";
import { cn } from "@/lib/utils";

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

  return (
    <div data-app-shell className="flex h-screen bg-shell-main text-foreground">
      <aside className="flex w-52 shrink-0 flex-col border-r border-border bg-shell-primary pt-[max(0.5rem,env(safe-area-inset-top))]">
        <div className="flex items-center gap-2 px-3 py-3 [padding-left:max(0.75rem,calc(var(--titlebar-macos-inset)-0.5rem))]">
          <BookMarked className="size-4 text-primary" />
          <span className="truncate text-sm font-semibold">Thread</span>
        </div>
        <Link
          to="/"
          className="mx-2 mb-2 flex items-center gap-2 rounded-md px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        >
          <Home className="size-3.5" />
          返回书架
        </Link>
        <nav className="flex flex-1 flex-col gap-0.5 px-2">
          {NAV.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              params={{ projectId }}
              className={cn(
                "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition hover:bg-accent hover:text-accent-foreground",
              )}
              activeProps={{
                className:
                  "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm bg-accent text-accent-foreground font-medium",
              }}
            >
              <item.icon className="size-4" />
              {item.label}
            </Link>
          ))}
        </nav>
      </aside>
      <div className="min-w-0 flex-1">
        <Outlet />
      </div>
    </div>
  );
}
