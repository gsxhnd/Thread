import { CopyIcon, ExternalLinkIcon, Loader2Icon, TriangleAlertIcon } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CodeEditor } from "@/components/editor/code-editor";
import type { HttpResponse } from "@/lib/api";
import { formatBytes, formatDuration, statusSurface } from "@/lib/format";
import { cn } from "@/lib/utils";
import { detectLanguage, languageFromContentType } from "@/lib/editor/monaco";
import { useResponseStore } from "@/stores/response-store";

/** 响应面板：状态 / Headers / Body（Monaco 只读）/ 时序。 */
export function ResponsePanel({ tabKey }: { tabKey: string }) {
  const state = useResponseStore((store) => store.byTab[tabKey]) ?? { status: "idle" };

  if (state.status === "idle") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
        <span className="text-sm font-medium text-foreground">等待响应</span>
        <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
          发送后在这里查看状态码、Headers、Body 与耗时。
        </p>
      </div>
    );
  }

  if (state.status === "loading") {
    return (
      <div className="flex h-full items-center justify-center gap-2 p-6 text-xs text-muted-foreground">
        <Loader2Icon className="size-4 animate-spin" />
        请求进行中，可在上方取消。
      </div>
    );
  }

  if (state.status === "error") {
    return (
      <div className="flex h-full items-start p-4">
        <div className="flex max-w-xl flex-col gap-2 rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-xs">
          <div className="flex items-center gap-2 text-destructive">
            <TriangleAlertIcon className="size-4" />
            <span className="font-medium">{state.code}</span>
          </div>
          <p className="break-words text-foreground">{state.message}</p>
        </div>
      </div>
    );
  }

  return <ResponseView response={state.response} />;
}

function ResponseView({ response }: { response: HttpResponse }) {
  const headerEntries = Object.entries(response.headers);
  const language = response.body
    ? (response.contentType
        ? languageFromContentType(response.contentType)
        : detectLanguage(response.body))
    : "plaintext";

  return (
    <Tabs defaultValue="body" className="flex min-h-0 flex-1 flex-col gap-0">
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border bg-shell-main px-3 py-2 text-xs">
        <span
          className={cn(
            "rounded-md px-1.5 py-0.5 font-semibold tabular-nums",
            statusSurface(response.statusCode),
          )}
        >
          {response.statusCode} {response.statusText}
        </span>
        <span className="font-mono text-muted-foreground tabular-nums">
          {formatDuration(response.timing.totalMs)}
        </span>
        <span className="font-mono text-muted-foreground tabular-nums">
          {formatBytes(response.bodySize)}
        </span>
        {response.contentType ? (
          <span className="truncate text-muted-foreground">{response.contentType}</span>
        ) : null}
        {response.redirectCount > 0 ? (
          <span className="text-muted-foreground">
            重定向 {response.redirectCount} 次
          </span>
        ) : null}

        <TabsList variant="line" className="ml-auto h-7">
          <TabsTrigger value="body">Body</TabsTrigger>
          <TabsTrigger value="headers">Headers ({headerEntries.length})</TabsTrigger>
          <TabsTrigger value="timing">时序</TabsTrigger>
        </TabsList>
      </div>

      {response.unresolvedVariables.length > 0 ? (
        <div className="flex items-start gap-2 border-b border-notice/30 bg-notice-bg px-3 py-1.5 text-[11px] text-notice-foreground">
          <TriangleAlertIcon className="mt-0.5 size-3.5 shrink-0" />
          <span>
            缺失变量：{response.unresolvedVariables.join("、")}
            —— 已在发送时保留原样，请在环境中补充。
          </span>
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-auto p-3">
        <TabsContent value="body" className="mt-0">
          {response.bodyFilePath ? (
            <div className="flex flex-col gap-2 rounded-xl border border-border bg-muted/40 p-3 text-xs">
              <p>
                响应体超过内联上限，已写入本地文件：
                <span className="ml-1 font-mono">{response.bodyFilePath}</span>
              </p>
              <p className="text-muted-foreground">
                大小 {formatBytes(response.bodySize)}（响应体保存在应用数据目录，不进入工作区）
              </p>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void revealItemInDir(response.bodyFilePath as string).catch((error) => {
                    toast.error(String(error));
                  });
                }}
              >
                <ExternalLinkIcon /> 在文件管理器中显示
              </Button>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <div className="flex justify-end">
                <Button
                  variant="ghost"
                  size="xs"
                  onClick={() => {
                    void navigator.clipboard
                      .writeText(response.body ?? "")
                      .then(() => toast.success("已复制响应体"))
                      .catch(() => toast.error("复制失败"));
                  }}
                >
                  <CopyIcon /> 复制
                </Button>
              </div>
              <CodeEditor
                value={response.body ?? ""}
                language={language}
                readOnly
                wordWrap
                minHeight={260}
              />
            </div>
          )}
        </TabsContent>

        <TabsContent value="headers" className="mt-0">
          <table className="w-full text-xs">
            <tbody>
              {headerEntries.map(([name, values]) => (
                <tr key={name} className="border-b border-border/70 align-top">
                  <td className="w-56 py-2 pr-3 font-mono text-muted-foreground">{name}</td>
                  <td className="py-2 font-mono break-all">{values.join("\n")}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </TabsContent>

        <TabsContent value="timing" className="mt-0">
          <div className="flex max-w-lg flex-col gap-2.5">
            <TimingBar label="DNS 解析" value={response.timing.dnsMs} total={response.timing.totalMs} />
            <TimingBar label="TCP 连接" value={response.timing.tcpMs} total={response.timing.totalMs} />
            <TimingBar label="TLS 握手" value={response.timing.tlsMs} total={response.timing.totalMs} />
            <TimingBar label="首字节" value={response.timing.ttfbMs} total={response.timing.totalMs} />
            <TimingBar label="内容下载" value={response.timing.downloadMs} total={response.timing.totalMs} />
            <TimingBar label="总计" value={response.timing.totalMs} total={response.timing.totalMs} />
          </div>
          <p className="mt-4 max-w-lg text-[11px] leading-relaxed text-muted-foreground">
            DNS / TCP 来自发送前的探测。阶段 1 不单独测量 TLS 握手，其耗时包含在首字节内。
          </p>
          <p className="mt-2 font-mono text-[11px] break-all text-muted-foreground">{response.url}</p>
        </TabsContent>
      </div>
    </Tabs>
  );
}

function TimingBar({ label, value, total }: { label: string; value: number; total: number }) {
  const width = total > 0 && value > 0 ? Math.max(4, Math.min(100, (value / total) * 100)) : 0;
  return (
    <div className="grid grid-cols-[5.5rem_minmax(0,1fr)_4.5rem] items-center gap-3 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <div className="h-1.5 overflow-hidden rounded-full bg-muted">
        <div className="h-full rounded-full bg-primary" style={{ width: `${width}%` }} />
      </div>
      <span className="text-right font-mono tabular-nums">{formatDuration(value)}</span>
    </div>
  );
}
