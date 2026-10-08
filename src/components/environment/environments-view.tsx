import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CircleCheckIcon,
  EyeIcon,
  PlusIcon,
  ServerIcon,
  Trash2Icon,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  envVariableDelete,
  envVariableReveal,
  envVariableSet,
  environmentActivate,
  environmentCreate,
  environmentDelete,
  environmentGet,
  environmentUpdate,
} from "@/lib/api";
import type { EnvVariableView } from "@/lib/api";
import { activeEnvironmentPath, useAppStore } from "@/stores/app-store";
import { PromptDialog } from "@/components/workspace/prompt-dialog";

/** 环境与变量管理：切换环境、增删改变量、机密值本地存储。 */
export function EnvironmentsView() {
  const snapshot = useAppStore((state) => state.snapshot);
  const refresh = useAppStore((state) => state.refresh);
  const setSnapshot = useAppStore((state) => state.setSnapshot);
  const queryClient = useQueryClient();

  const environments = snapshot?.index.environments ?? [];
  const activePath = activeEnvironmentPath(snapshot);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [newSecret, setNewSecret] = useState(false);

  useEffect(() => {
    if (!selectedPath && environments.length > 0) {
      setSelectedPath(activePath ?? environments[0]?.path ?? null);
    }
  }, [selectedPath, environments, activePath]);

  const currentPath = selectedPath ?? activePath ?? environments[0]?.path ?? null;

  const { data: detail } = useQuery({
    queryKey: ["environment", currentPath],
    queryFn: () => environmentGet(currentPath as string),
    enabled: !!currentPath,
  });

  const reload = async () => {
    await queryClient.invalidateQueries({ queryKey: ["environment"] });
    await refresh();
  };

  const handleActivate = async () => {
    if (!currentPath) return;
    const next = await environmentActivate(currentPath);
    setSnapshot(next);
    toast.success(`已切换到 ${detail?.name ?? currentPath}`);
  };

  const handleDeleteEnvironment = async () => {
    if (!currentPath) return;
    try {
      const result = await environmentDelete(currentPath);
      setSelectedPath(null);
      await reload();
      toast.success(`已移入回收站：${result.trashedPath}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const handleAddVariable = async () => {
    if (!currentPath || !newKey.trim()) return;
    try {
      await envVariableSet({
        environmentPath: currentPath,
        key: newKey.trim(),
        value: newValue,
        enabled: true,
        secret: newSecret,
      });
      setNewKey("");
      setNewValue("");
      setNewSecret(false);
      await reload();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <div className="flex min-h-0 flex-1">
      <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-shell-secondary">
        <header className="flex items-center gap-2 border-b border-border px-3 py-3">
          <ServerIcon className="size-4 text-muted-foreground" />
          <div className="min-w-0">
            <h2 className="text-sm font-medium">环境</h2>
            <p className="text-[11px] text-muted-foreground">写入工作区文件</p>
          </div>
          <Button
            variant="ghost"
            size="icon-xs"
            className="ml-auto"
            title="新建环境"
            onClick={() => setCreating(true)}
          >
            <PlusIcon />
          </Button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {environments.length === 0 ? (
            <p className="p-2 text-xs text-muted-foreground">
              还没有环境。新建后会写入 environments/*.env.json。
            </p>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {environments.map((environment) => (
                <li key={environment.path}>
                  <button
                    type="button"
                    className={`flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-sm ${
                      currentPath === environment.path
                        ? "bg-accent text-accent-foreground"
                        : "hover:bg-muted"
                    }`}
                    onClick={() => setSelectedPath(environment.path)}
                  >
                    <span className="min-w-0 flex-1 truncate">
                      {environment.name}
                      {environment.isGlobal ? " · 全局" : ""}
                    </span>
                    {environment.isActive ? (
                      <CircleCheckIcon className="size-3.5 text-primary" />
                    ) : null}
                    <span className="text-[11px] text-muted-foreground">
                      {environment.variableCount}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </aside>

      <section className="flex min-h-0 flex-1 flex-col">
        {detail ? (
          <>
            <header className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-4 py-3">
              <Input
                className="w-56"
                defaultValue={detail.name}
                key={`${detail.path}-name`}
                onBlur={(event) => {
                  const value = event.target.value.trim();
                  if (value && value !== detail.name) {
                    void environmentUpdate({ path: detail.path, name: value }).then(reload);
                  }
                }}
              />
              <Button
                variant={detail.isActive ? "secondary" : "outline"}
                size="sm"
                disabled={detail.isActive}
                onClick={() => void handleActivate()}
              >
                {detail.isActive ? "当前环境" : "切换到此环境"}
              </Button>
              <span className="font-mono text-[11px] text-muted-foreground">{detail.path}</span>
              <Button
                variant="ghost"
                size="sm"
                className="ml-auto"
                disabled={detail.isGlobal}
                title={detail.isGlobal ? "全局环境不可删除" : "删除环境（移入回收站）"}
                onClick={() => void handleDeleteEnvironment()}
              >
                <Trash2Icon /> 删除
              </Button>
            </header>

            <div className="min-h-0 flex-1 overflow-y-auto p-3">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b border-border text-left text-muted-foreground">
                    <th className="w-10 py-1.5">启用</th>
                    <th className="w-52 py-1.5">键</th>
                    <th className="py-1.5">值</th>
                    <th className="w-16 py-1.5 text-center">机密</th>
                    <th className="w-20 py-1.5" />
                  </tr>
                </thead>
                <tbody>
                  {detail.variables.map((variable) => (
                    <VariableRow
                      key={variable.key}
                      variable={variable}
                      environmentPath={detail.path}
                      onChanged={reload}
                    />
                  ))}
                </tbody>
              </table>

              <div className="mt-4 flex flex-wrap items-end gap-2 rounded-xl border border-dashed border-border bg-muted/40 p-3">
                <label className="flex flex-col gap-1">
                  <span className="text-[11px] text-muted-foreground">键</span>
                  <Input
                    className="w-44 font-mono text-xs"
                    value={newKey}
                    placeholder="apiKey"
                    onChange={(event) => setNewKey(event.target.value)}
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-[11px] text-muted-foreground">值</span>
                  <Input
                    className="w-72 font-mono text-xs"
                    type={newSecret ? "password" : "text"}
                    value={newValue}
                    placeholder="支持任意文本"
                    onChange={(event) => setNewValue(event.target.value)}
                  />
                </label>
                <label className="flex items-center gap-1.5 pb-1.5 text-xs">
                  <input
                    type="checkbox"
                    className="size-3.5 accent-primary"
                    checked={newSecret}
                    onChange={(event) => setNewSecret(event.target.checked)}
                  />
                  机密
                </label>
                <Button
                  size="sm"
                  disabled={!newKey.trim()}
                  onClick={() => void handleAddVariable()}
                >
                  <PlusIcon /> 添加变量
                </Button>
              </div>

              <p className="mt-3 max-w-2xl text-[11px] text-muted-foreground">
                机密变量的值不会写入工作区文件（文件里只有占位），而是保存在本地数据目录，
                因此不会被 Git 提交或分享。需要查看时可点击行内的眼睛按钮显式读取。
              </p>
            </div>
          </>
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
            <span className="text-sm font-medium">还没有选中环境</span>
            <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
              新建一个环境后，变量会写入 environments/*.env.json。
            </p>
          </div>
        )}
      </section>

      {creating ? (
        <PromptDialog
          open
          title="新建环境"
          label="环境名称"
          confirmText="创建"
          defaultValue="dev"
          onOpenChange={(open) => {
            if (!open) setCreating(false);
          }}
          onConfirm={async (value) => {
            const created = await environmentCreate(value);
            await reload();
            setSelectedPath(created.path);
          }}
        />
      ) : null}
    </div>
  );
}

function VariableRow({
  variable,
  environmentPath,
  onChanged,
}: {
  variable: EnvVariableView;
  environmentPath: string;
  onChanged: () => Promise<void>;
}) {
  const [value, setValue] = useState(variable.value);
  const [revealed, setRevealed] = useState<string | null>(null);

  useEffect(() => {
    setValue(variable.value);
    setRevealed(null);
  }, [variable.value, variable.key]);

  const commitValue = async (next: string) => {
    if (!variable.secret && next === variable.value) return;
    if (variable.secret && next === "") return;
    await envVariableSet({
      environmentPath,
      key: variable.key,
      value: next,
    });
    await onChanged();
  };

  const handleReveal = async () => {
    const stored = await envVariableReveal({ environmentPath, key: variable.key });
    setRevealed(stored ?? "");
    toast.message(`${variable.key} 的值`, { description: stored ?? "(空)" });
  };

  return (
    <tr className="border-b border-border/60">
      <td className="py-1">
        <input
          type="checkbox"
          className="size-3.5 accent-primary"
          checked={variable.enabled}
          onChange={(event) => {
            void envVariableSet({
              environmentPath,
              key: variable.key,
              enabled: event.target.checked,
            }).then(onChanged);
          }}
        />
      </td>
      <td className="py-1 pr-2 font-mono">{variable.key}</td>
      <td className="py-1 pr-2">
        <Input
          className="h-7 font-mono text-xs"
          type={variable.secret && revealed === null ? "password" : "text"}
          value={revealed ?? value}
          placeholder={variable.secret && variable.hasStoredValue ? "已保存（本地）" : ""}
          onChange={(event) => {
            setRevealed(null);
            setValue(event.target.value);
          }}
          onBlur={(event) => void commitValue(event.target.value)}
        />
      </td>
      <td className="py-1 text-center">
        <input
          type="checkbox"
          className="size-3.5 accent-primary"
          checked={variable.secret}
          onChange={(event) => {
            void envVariableSet({
              environmentPath,
              key: variable.key,
              secret: event.target.checked,
            }).then(onChanged);
          }}
        />
      </td>
      <td className="py-1">
        <div className="flex justify-end gap-1">
          {variable.secret ? (
            <Button
              variant="ghost"
              size="icon-xs"
              title="显示已保存的机密值"
              onClick={() => void handleReveal()}
            >
              <EyeIcon />
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="icon-xs"
            title="删除变量"
            onClick={() => {
              void envVariableDelete({
                environmentPath,
                key: variable.key,
              }).then(onChanged);
            }}
          >
            <Trash2Icon />
          </Button>
        </div>
      </td>
    </tr>
  );
}
