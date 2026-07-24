import { useMutation, useQuery } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { useParams } from "@tanstack/react-router";
import { Download, KeyRound } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  aiConfigGet,
  aiConfigSet,
  exportRun,
  onExportProgress,
} from "@/lib/api";
import { ApiError } from "@/lib/api/client";
import type { ExportFormat } from "@/lib/api/types";

export function SettingsPage() {
  const { projectId } = useParams({ from: "/project/$projectId/settings" });
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com");
  const [model, setModel] = useState("gpt-4o-mini");
  const [apiKey, setApiKey] = useState("");
  const [progress, setProgress] = useState<number | null>(null);

  const { data: aiCfg } = useQuery({
    queryKey: ["aiConfig"],
    queryFn: aiConfigGet,
  });

  useEffect(() => {
    if (!aiCfg) return;
    if (aiCfg.baseUrl) setBaseUrl(aiCfg.baseUrl);
    if (aiCfg.model) setModel(aiCfg.model);
  }, [aiCfg]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onExportProgress((p) => setProgress(p.percent)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const saveAi = useMutation({
    mutationFn: () => aiConfigSet({ baseUrl, apiKey, model }),
    onSuccess: () => {
      setApiKey("");
      toast.success("AI 配置已保存（Key 不会回传前端）");
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "失败"),
  });

  const doExport = async (format: ExportFormat) => {
    const ext =
      format === "md"
        ? "md"
        : format === "txt"
          ? "txt"
          : format === "epub"
            ? "epub"
            : "docx";
    const targetPath = await save({
      defaultPath: `export.${ext}`,
      filters: [{ name: format.toUpperCase(), extensions: [ext] }],
    });
    if (!targetPath) return;
    try {
      setProgress(0);
      const res = await exportRun({
        projectId,
        format,
        scope: { kind: "all" },
        targetPath,
      });
      toast.success(`已导出到 ${res.path}`);
    } catch (e) {
      toast.error(e instanceof ApiError ? e.message : "导出失败");
    } finally {
      setProgress(null);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-6 py-4">
        <h1 className="text-lg font-semibold">设置</h1>
        <p className="text-sm text-muted-foreground">AI 配置与作品导出</p>
      </header>
      <div className="flex-1 space-y-8 overflow-auto p-6">
        <section className="max-w-xl space-y-3">
          <div className="flex items-center gap-2 text-sm font-semibold">
            <KeyRound className="size-4" />
            AI（OpenAI 兼容 · BYOK）
          </div>
          <Input
            placeholder="Base URL"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
          />
          <Input
            placeholder="Model"
            value={model}
            onChange={(e) => setModel(e.target.value)}
          />
          <Input
            type="password"
            placeholder={
              aiCfg?.hasKey ? "已配置 Key（输入以更新）" : "API Key"
            }
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
          />
          <Button
            disabled={!apiKey.trim() || !baseUrl.trim() || !model.trim()}
            onClick={() => saveAi.mutate()}
          >
            保存 AI 配置
          </Button>
        </section>

        <section className="max-w-xl space-y-3">
          <div className="flex items-center gap-2 text-sm font-semibold">
            <Download className="size-4" />
            导出整本
          </div>
          <div className="flex flex-wrap gap-2">
            {(["txt", "md", "epub", "docx"] as ExportFormat[]).map((fmt) => (
              <Button
                key={fmt}
                variant="outline"
                onClick={() => void doExport(fmt)}
              >
                {fmt.toUpperCase()}
              </Button>
            ))}
          </div>
          {progress !== null && (
            <p className="text-xs text-muted-foreground">导出进度 {progress}%</p>
          )}
        </section>
      </div>
    </div>
  );
}
