import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { useNavigate } from "@tanstack/react-router";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  BracesIcon,
  FolderGit2Icon,
  FolderOpenIcon,
  Loader2Icon,
  PlusIcon,
  SendIcon,
  XIcon,
} from "lucide-react";
import { BrandMark } from "@/components/brand-mark";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { WindowTitleBar } from "@/components/window-title-bar";
import {
  appPaths,
  getConfigLoadWarnings,
  sessionForgetWorkspace,
  sessionGet,
} from "@/lib/api";
import type { AppPaths, RecentWorkspace } from "@/lib/api";
import { useAppStore } from "@/stores/app-store";

type Mode = "create" | "open";

export function WelcomePage() {
  const navigate = useNavigate();
  const { openWorkspace, loading, error, clearError, snapshot } = useAppStore();
  const [recent, setRecent] = useState<RecentWorkspace[]>([]);
  const [mode, setMode] = useState<Mode>("create");
  const [name, setName] = useState("My Workspace");
  const [path, setPath] = useState("");
  const [paths, setPaths] = useState<AppPaths | null>(null);

  const loadSession = useCallback(async () => {
    const session = await sessionGet();
    setRecent(session.recentWorkspaces);
  }, []);

  useEffect(() => {
    void loadSession();
    void getConfigLoadWarnings().then((warnings) => {
      for (const warning of warnings) {
        toast.warning("配置已恢复默认", { description: warning });
      }
    });
    void appPaths().then(setPaths);
  }, [loadSession]);

  // Rust 侧可能在启动时恢复了上次的工作区。
  useEffect(() => {
    if (snapshot) void navigate({ to: "/workspace" });
  }, [snapshot, navigate]);

  const pickDirectory = async () => {
    const selected = await openDialog({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setPath(selected);
    }
  };

  const handleSubmit = async () => {
    clearError();
    await openWorkspace(path, name, mode);
    await navigate({ to: "/workspace" });
  };

  const handleOpenRecent = async (item: RecentWorkspace) => {
    clearError();
    await openWorkspace(item.path, item.name, "open");
    await navigate({ to: "/workspace" });
  };

  const handleForget = async (item: RecentWorkspace) => {
    const session = await sessionForgetWorkspace(item.path);
    setRecent(session.recentWorkspaces);
  };

  return (
    <div data-app-root className="flex min-h-screen flex-col bg-shell-main text-foreground">
      <WindowTitleBar>
        <span className="flex size-5 items-center justify-center rounded-md bg-primary text-primary-foreground">
          <BrandMark className="size-3.5" />
        </span>
        <span className="text-sm font-medium">Thread</span>
      </WindowTitleBar>

      <div className="welcome-canvas flex min-h-0 flex-1 items-center justify-center px-6 py-8">
        <div className="grid w-full max-w-5xl items-start gap-10 lg:grid-cols-[minmax(0,0.85fr)_minmax(0,1.15fr)]">
          <div className="flex flex-col gap-8 pt-2">
            <div className="flex flex-col gap-4">
              <span className="flex size-12 items-center justify-center rounded-2xl bg-primary text-primary-foreground">
                <BrandMark className="size-7" />
              </span>
              <div className="flex flex-col gap-2">
                <p className="text-xs font-medium tracking-[0.22em] text-muted-foreground uppercase">
                  Thread
                </p>
                <h1 className="text-4xl font-semibold tracking-tight">灰线</h1>
                <p className="max-w-sm text-sm leading-relaxed text-muted-foreground">
                  草蛇灰线，伏脉千里。本地优先的 API 工作台：集合、请求与环境都是工作区里的文件，发送在本机完成。
                </p>
              </div>
            </div>

            <ul className="flex max-w-sm flex-col gap-3">
              <Feature
                icon={FolderGit2Icon}
                title="工作区即仓库"
                detail="JSON 文件可直接用 Git 跟踪、diff 与评审。"
              />
              <Feature
                icon={BracesIcon}
                title="环境变量"
                detail="用 {{variable}} 切换环境，机密值留在本地。"
              />
              <Feature
                icon={SendIcon}
                title="HTTP 请求闭环"
                detail="编辑、发送、看响应，再保存回集合。"
              />
            </ul>

            {paths ? (
              <footer className="flex flex-col gap-1 text-[11px] text-muted-foreground">
                <span className="truncate" title={paths.dataDir}>
                  本地数据 {paths.dataDir}
                </span>
                <span className="truncate" title={paths.configFile}>
                  配置 {paths.configFile}
                </span>
              </footer>
            ) : null}
          </div>

          <div className="flex flex-col gap-4">
            <section className="flex flex-col gap-4 rounded-2xl border border-border bg-card p-5">
              <div className="flex rounded-lg bg-muted p-1">
                <Button
                  variant={mode === "create" ? "default" : "ghost"}
                  size="sm"
                  className="flex-1"
                  onClick={() => setMode("create")}
                >
                  <PlusIcon data-icon="inline-start" />
                  新建工作区
                </Button>
                <Button
                  variant={mode === "open" ? "default" : "ghost"}
                  size="sm"
                  className="flex-1"
                  onClick={() => setMode("open")}
                >
                  <FolderOpenIcon data-icon="inline-start" />
                  打开工作区
                </Button>
              </div>

              <div className="flex flex-col gap-3">
                {mode === "create" ? (
                  <label className="flex flex-col gap-1.5">
                    <span className="text-xs font-medium text-muted-foreground">工作区名称</span>
                    <Input
                      value={name}
                      onChange={(event) => setName(event.target.value)}
                      placeholder="My Workspace"
                    />
                  </label>
                ) : null}

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-muted-foreground">
                    {mode === "create" ? "存放目录" : "工作区目录"}
                  </span>
                  <div className="flex gap-2">
                    <Input
                      value={path}
                      onChange={(event) => setPath(event.target.value)}
                      placeholder="/Users/you/workspaces/my-api"
                    />
                    <Button variant="outline" onClick={pickDirectory}>
                      选择…
                    </Button>
                  </div>
                  <span className="text-xs text-muted-foreground">
                    目录里会生成 thread.workspace.json、集合与 environments。
                  </span>
                </label>
              </div>

              {error ? (
                <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
                  {error}
                </p>
              ) : null}

              <Button
                onClick={handleSubmit}
                disabled={loading || !path.trim() || (mode === "create" && !name.trim())}
              >
                {loading ? <Loader2Icon data-icon="inline-start" className="animate-spin" /> : null}
                {mode === "create" ? "创建并打开" : "打开工作区"}
              </Button>
            </section>

            <section className="flex flex-col gap-2 rounded-2xl border border-border bg-card p-5">
              <h2 className="text-sm font-medium">最近打开</h2>
              {recent.length === 0 ? (
                <p className="py-3 text-xs text-muted-foreground">还没有打开过工作区。</p>
              ) : (
                <ul className="flex flex-col gap-0.5">
                  {recent.map((item) => (
                    <li key={item.path}>
                      <div className="group flex items-center gap-2 rounded-lg px-2 py-2 hover:bg-muted">
                        <FolderOpenIcon className="size-4 shrink-0 text-muted-foreground" />
                        <button
                          type="button"
                          className="min-w-0 flex-1 text-left"
                          onClick={() => void handleOpenRecent(item)}
                        >
                          <span className="block truncate text-sm">{item.name}</span>
                          <span className="block truncate text-xs text-muted-foreground">
                            {item.path}
                          </span>
                        </button>
                        <Button
                          variant="ghost"
                          size="icon-xs"
                          className="opacity-0 group-hover:opacity-100"
                          title="从列表移除"
                          onClick={() => void handleForget(item)}
                        >
                          <XIcon />
                        </Button>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}

function Feature({
  icon: Icon,
  title,
  detail,
}: {
  icon: typeof SendIcon;
  title: string;
  detail: string;
}) {
  return (
    <li className="flex items-start gap-3">
      <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent text-accent-foreground">
        <Icon className="size-4" />
      </span>
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="text-sm font-medium">{title}</span>
        <span className="text-xs leading-relaxed text-muted-foreground">{detail}</span>
      </span>
    </li>
  );
}
