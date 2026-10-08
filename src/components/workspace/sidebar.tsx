import { useState } from "react";
import {
  FolderPlusIcon,
  HistoryIcon,
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
  RefreshCwIcon,
  SearchIcon,
  SendIcon,
  ServerIcon,
  SettingsIcon,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { collectionCreate } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app-store";
import { useUiStore, type WorkspaceView } from "@/stores/ui-store";
import { CollectionTree } from "./collection-tree";
import { PromptDialog } from "./prompt-dialog";

const NAV_ITEMS: { view: WorkspaceView; label: string; hint: string; icon: typeof SendIcon }[] = [
  { view: "request", label: "请求", hint: "编辑并发送集合里的请求", icon: SendIcon },
  { view: "history", label: "历史", hint: "发送记录保存在本地，不会进入 Git", icon: HistoryIcon },
  { view: "environments", label: "环境", hint: "变量写入 environments/*.env.json", icon: ServerIcon },
  { view: "settings", label: "设置", hint: "应用默认值与工作区配置", icon: SettingsIcon },
];

export function Sidebar() {
  const snapshot = useAppStore((state) => state.snapshot);
  const refresh = useAppStore((state) => state.refresh);
  const view = useUiStore((state) => state.view);
  const setView = useUiStore((state) => state.setView);
  const search = useUiStore((state) => state.search);
  const setSearch = useUiStore((state) => state.setSearch);
  const collapsed = useUiStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useUiStore((state) => state.toggleSidebar);
  const [creating, setCreating] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  if (!snapshot) return null;

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await refresh();
      toast.success("已重新扫描工作区");
    } finally {
      setRefreshing(false);
    }
  };

  if (collapsed) {
    return (
      <aside className="flex w-12 shrink-0 flex-col items-center gap-1 border-r border-border bg-shell-secondary py-2">
        <Button variant="ghost" size="icon-sm" title="展开侧栏" onClick={toggleSidebar}>
          <PanelLeftOpenIcon />
        </Button>
        <nav className="mt-1 flex flex-col gap-1">
          {NAV_ITEMS.map((item) => (
            <Button
              key={item.view}
              variant={view === item.view ? "secondary" : "ghost"}
              size="icon-sm"
              title={item.label}
              onClick={() => setView(item.view)}
            >
              <item.icon />
            </Button>
          ))}
        </nav>
        <Button
          variant="ghost"
          size="icon-sm"
          className="mt-auto"
          title="重新扫描工作区"
          onClick={() => void handleRefresh()}
        >
          <RefreshCwIcon className={refreshing ? "animate-spin" : undefined} />
        </Button>
        {creating ? (
          <NewCollectionDialog onOpenChange={setCreating} />
        ) : null}
      </aside>
    );
  }

  return (
    <aside className="flex w-72 shrink-0 flex-col border-r border-border bg-shell-secondary">
      <div className="flex items-start gap-2 px-3 pt-3 pb-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{snapshot.index.name}</p>
          <p className="truncate text-[11px] text-muted-foreground">
            {snapshot.index.requestCount} 个请求 · {snapshot.index.collectionCount} 个集合
          </p>
        </div>
        <Button
          variant="ghost"
          size="icon-xs"
          title="重新扫描工作区（外部修改 / Git 切换分支后）"
          onClick={() => void handleRefresh()}
        >
          <RefreshCwIcon className={refreshing ? "animate-spin" : undefined} />
        </Button>
        <Button variant="ghost" size="icon-xs" title="折叠侧栏" onClick={toggleSidebar}>
          <PanelLeftCloseIcon />
        </Button>
      </div>

      <nav className="flex flex-col gap-0.5 px-2 pb-2">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const active = view === item.view;
          return (
            <button
              key={item.view}
              type="button"
              className={cn(
                "flex items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm",
                active
                  ? "bg-accent font-medium text-accent-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
              onClick={() => setView(item.view)}
            >
              <Icon className="size-4 shrink-0" />
              {item.label}
            </button>
          );
        })}
      </nav>

      {view === "request" ? (
        <>
          <div className="flex flex-col gap-2 border-t border-border px-3 py-3">
            <div className="relative">
              <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="搜索请求、URL 或路径"
                className="pl-8"
              />
            </div>
            <Button variant="outline" size="sm" onClick={() => setCreating(true)}>
              <FolderPlusIcon data-icon="inline-start" />
              新建集合
            </Button>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
            <CollectionTree />
          </div>
        </>
      ) : (
        <div className="min-h-0 flex-1 px-4 py-3">
          <p className="text-xs leading-relaxed text-muted-foreground">
            {NAV_ITEMS.find((item) => item.view === view)?.hint}
          </p>
        </div>
      )}

      {creating ? <NewCollectionDialog onOpenChange={setCreating} /> : null}
    </aside>
  );
}

function NewCollectionDialog({ onOpenChange }: { onOpenChange: (open: boolean) => void }) {
  const refresh = useAppStore((state) => state.refresh);

  return (
    <PromptDialog
      open
      title="新建集合"
      label="集合名称"
      confirmText="创建"
      defaultValue="New Collection"
      onOpenChange={(open) => {
        if (!open) onOpenChange(false);
      }}
      onConfirm={async (value) => {
        await collectionCreate({ name: value });
        await refresh();
      }}
    />
  );
}
