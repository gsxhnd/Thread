import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { SaveIcon, SettingsIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  appPaths,
  appSettingsGet,
  appSettingsSet,
  workspaceUpdateSettings,
} from "@/lib/api";
import type { AppSettings } from "@/lib/api";
import { useAppStore } from "@/stores/app-store";

/** 应用与工作区设置。 */
export function SettingsView() {
  const snapshot = useAppStore((state) => state.snapshot);
  const setSnapshot = useAppStore((state) => state.setSnapshot);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [workspaceName, setWorkspaceName] = useState(snapshot?.config.name ?? "");
  const [proxyPort, setProxyPort] = useState(snapshot?.config.proxy.port ?? 9090);

  const { data: paths } = useQuery({
    queryKey: ["app-paths"],
    queryFn: () => appPaths(),
  });

  useEffect(() => {
    void appSettingsGet().then(setSettings);
  }, []);

  useEffect(() => {
    setWorkspaceName(snapshot?.config.name ?? "");
    setProxyPort(snapshot?.config.proxy.port ?? 9090);
  }, [snapshot?.config.name, snapshot?.config.proxy.port]);

  const handleSaveAppSettings = async () => {
    if (!settings) return;
    const saved = await appSettingsSet(settings);
    setSettings(saved);
    toast.success("已保存应用设置（config.toml）");
  };

  const handleSaveWorkspace = async () => {
    const next = await workspaceUpdateSettings({
      name: workspaceName,
      proxy: { port: proxyPort, autoStart: snapshot?.config.proxy.autoStart ?? false },
    });
    setSnapshot(next);
    toast.success("已写入 thread.workspace.json");
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
      <header className="mb-5 flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <SettingsIcon className="size-4 text-muted-foreground" />
          <h2 className="text-sm font-medium">设置</h2>
        </div>
        <p className="text-xs text-muted-foreground">
          应用默认值写在 config.toml；工作区名称与代理端口写在 thread.workspace.json。
        </p>
      </header>

      <div className="grid max-w-4xl gap-4 lg:grid-cols-2">
        <section className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4">
          <h3 className="text-sm font-medium">请求默认值（应用级）</h3>
          {settings ? (
            <>
              <label className="flex flex-col gap-1">
                <span className="text-[11px] text-muted-foreground">默认超时（毫秒）</span>
                <Input
                  type="number"
                  min={1}
                  value={settings.defaultTimeoutMs}
                  onChange={(event) =>
                    setSettings({
                      ...settings,
                      defaultTimeoutMs: Number(event.target.value) || 1,
                    })
                  }
                />
              </label>
              <label className="flex items-center gap-2 text-xs">
                <input
                  type="checkbox"
                  className="size-3.5 accent-primary"
                  checked={settings.followRedirects}
                  onChange={(event) =>
                    setSettings({ ...settings, followRedirects: event.target.checked })
                  }
                />
                默认跟随重定向
              </label>
              <label className="flex items-center gap-2 text-xs">
                <input
                  type="checkbox"
                  className="size-3.5 accent-primary"
                  checked={settings.verifySsl}
                  onChange={(event) =>
                    setSettings({ ...settings, verifySsl: event.target.checked })
                  }
                />
                默认校验证书
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-[11px] text-muted-foreground">
                  响应体内联上限（字节，超过则落盘到本地数据目录）
                </span>
                <Input
                  type="number"
                  min={0}
                  value={settings.maxInlineBodyBytes}
                  onChange={(event) =>
                    setSettings({
                      ...settings,
                      maxInlineBodyBytes: Number(event.target.value) || 0,
                    })
                  }
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-[11px] text-muted-foreground">
                  历史保留条数（0 表示不限制）
                </span>
                <Input
                  type="number"
                  min={0}
                  value={settings.historyLimit}
                  onChange={(event) =>
                    setSettings({
                      ...settings,
                      historyLimit: Number(event.target.value) || 0,
                    })
                  }
                />
              </label>
              <Button size="sm" onClick={() => void handleSaveAppSettings()}>
                <SaveIcon /> 保存应用设置
              </Button>
            </>
          ) : (
            <p className="text-xs text-muted-foreground">加载中…</p>
          )}
        </section>

        <section className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4">
          <h3 className="text-sm font-medium">工作区</h3>
          <label className="flex flex-col gap-1">
            <span className="text-[11px] text-muted-foreground">工作区名称</span>
            <Input
              value={workspaceName}
              onChange={(event) => setWorkspaceName(event.target.value)}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-[11px] text-muted-foreground">
              代理端口（阶段 3 使用）
            </span>
            <Input
              type="number"
              min={1}
              value={proxyPort}
              onChange={(event) => setProxyPort(Number(event.target.value) || 9090)}
            />
          </label>
          {snapshot ? (
            <dl className="flex flex-col gap-1 text-[11px] text-muted-foreground">
              <div>工作区目录：<span className="font-mono">{snapshot.index.root}</span></div>
              <div>
                集合 {snapshot.index.collectionCount} 个 · 请求 {snapshot.index.requestCount} 个 ·
                环境 {snapshot.index.environments.length} 个
              </div>
              <div>索引摘要：<span className="font-mono">{snapshot.index.digest}</span></div>
            </dl>
          ) : null}
          <Button size="sm" onClick={() => void handleSaveWorkspace()}>
            <SaveIcon /> 保存工作区设置
          </Button>
        </section>

        <section className="flex flex-col gap-2 rounded-xl border border-border bg-card p-4">
          <h3 className="text-sm font-medium">数据位置</h3>
          <ul className="flex flex-col gap-1 font-mono text-[11px] break-all text-muted-foreground">
            <li>配置：{paths?.configFile ?? "-"}</li>
            <li>本地数据：{paths?.dataDir ?? "-"}</li>
            <li>数据库：{paths?.dbFile ?? "-"}</li>
          </ul>
          <p className="text-[11px] text-muted-foreground">
            集合/请求/环境是工作区内的 JSON 文件（可 Git 跟踪）；历史、大响应体、
            机密变量值保存在本地数据目录，不会进入 Git。
          </p>
        </section>
      </div>
    </div>
  );
}
