import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  HistoryIcon,
  RefreshCwIcon,
  RotateCcwIcon,
  SaveIcon,
  Trash2Icon,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { CodeEditor } from "@/components/editor/code-editor";
import {
  historyClear,
  historyDelete,
  historyGet,
  historyList,
  historySaveToCollection,
  requestGet,
} from "@/lib/api";
import type { HistorySummary } from "@/lib/api";
import { formatBytes, formatDuration, formatRelativeTime, methodColor, statusSurface } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app-store";
import { useTabsStore } from "@/stores/tabs-store";
import { useUiStore } from "@/stores/ui-store";
import { useSendRequest } from "@/components/request/use-send-request";

/** 请求历史（本地数据库，不进 Git）：重发、保存到集合、清理。 */
export function HistoryView() {
  const queryClient = useQueryClient();
  const refresh = useAppStore((state) => state.refresh);
  const selectedCollectionPath = useUiStore((state) => state.selectedCollectionPath);
  const setView = useUiStore((state) => state.setView);
  const { send } = useSendRequest();
  const [detailId, setDetailId] = useState<string | null>(null);

  const { data, isFetching, refetch } = useQuery({
    queryKey: ["history"],
    queryFn: () => historyList({ limit: 200 }),
  });

  const { data: detail } = useQuery({
    queryKey: ["history-detail", detailId],
    queryFn: () => historyGet(detailId as string),
    enabled: !!detailId,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => historyDelete(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["history"] });
      toast.success("已删除该条历史");
    },
  });

  const handleClear = async () => {
    const result = await historyClear();
    await queryClient.invalidateQueries({ queryKey: ["history"] });
    toast.success(`已清理 ${result.deletedCount} 条历史`);
  };

  const handleResend = async (entry: HistorySummary) => {
    const full = await historyGet(entry.id);
    const snapshot = full.requestSnapshot;
    const document = snapshot.document;

    // 请求文件仍存在时直接打开该请求，否则用快照开一个草稿标签页。
    if (entry.requestFilePath) {
      try {
        const current = await requestGet(entry.requestFilePath);
        useTabsStore.getState().openRequest({
          path: current.path,
          name: current.name,
          protocol: current.protocol,
          collectionPath: current.collectionPath,
          document: current.raw,
        });
        setView("request");
        await send(current.path);
        return;
      } catch {
        // 文件已被删除/移动，退回到快照方式。
      }
    }

    if (!document) {
      toast.error("该历史记录没有可重发的请求快照");
      return;
    }

    useTabsStore.getState().openDraft({
      collectionPath: selectedCollectionPath ?? "",
      name: snapshot.name ?? entry.name ?? "History Replay",
    });
    const opened = useTabsStore.getState().tabs;
    const draft = opened[opened.length - 1];
    if (!draft) return;
    useTabsStore.getState().updateDocument(draft.key, { ...document, name: draft.name });
    setView("request");
    await send(draft.key);
  };

  const handleSaveToCollection = async (entry: HistorySummary) => {
    if (!selectedCollectionPath) {
      toast.error("请先在左侧选择一个集合");
      return;
    }
    const created = await historySaveToCollection({
      id: entry.id,
      collectionPath: selectedCollectionPath,
      name: entry.name ?? undefined,
    });
    await refresh();
    useTabsStore.getState().openRequest({
      path: created.path,
      name: created.name,
      protocol: created.protocol,
      collectionPath: created.collectionPath,
      document: created.raw,
    });
    setView("request");
    toast.success("已保存为集合请求");
  };

  const items = data?.items ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex shrink-0 items-center gap-3 border-b border-border px-4 py-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="flex items-center gap-2">
            <HistoryIcon className="size-4 text-muted-foreground" />
            <h2 className="text-sm font-medium">请求历史</h2>
          </div>
          <p className="text-xs text-muted-foreground">
            共 {data?.total ?? 0} 条，保存在本地数据目录，不进入 Git
          </p>
        </div>
        <div className="ml-auto flex gap-2">
          <Button variant="ghost" size="sm" onClick={() => void refetch()}>
            <RefreshCwIcon className={isFetching ? "animate-spin" : undefined} /> 刷新
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={items.length === 0}
            onClick={() => void handleClear()}
          >
            <Trash2Icon /> 清理全部
          </Button>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {items.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
            <span className="text-sm font-medium">还没有历史</span>
            <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
              发送请求后会自动记在这里，可以重发、保存到集合，或清理。
            </p>
          </div>
        ) : (
          <table className="w-full text-xs">
            <tbody>
              {items.map((entry) => (
                <tr key={entry.id} className="group border-b border-border/70 hover:bg-muted/50">
                  <td
                    className={cn(
                      "w-16 py-2.5 pl-4 text-[10px] font-semibold tracking-wide uppercase",
                      methodColor(entry.method),
                    )}
                  >
                    {entry.method ?? entry.protocol}
                  </td>
                  <td className="max-w-0 py-2.5">
                    <button
                      type="button"
                      className="block w-full truncate text-left text-sm hover:underline"
                      title={entry.url ?? ""}
                      onClick={() => setDetailId(entry.id)}
                    >
                      {entry.name ?? entry.url ?? "(未命名)"}
                    </button>
                    <span className="block truncate font-mono text-[11px] text-muted-foreground">
                      {entry.url}
                    </span>
                  </td>
                  <td className="w-16 py-2.5 text-right">
                    <span
                      className={cn(
                        "inline-flex rounded-md px-1.5 py-0.5 font-semibold tabular-nums",
                        statusSurface(entry.statusCode ?? 0),
                      )}
                    >
                      {entry.statusCode ?? "-"}
                    </span>
                  </td>
                  <td className="w-20 py-2.5 text-right font-mono text-muted-foreground tabular-nums">
                    {entry.durationMs != null ? formatDuration(entry.durationMs) : "-"}
                  </td>
                  <td className="w-24 py-2.5 text-right text-muted-foreground">
                    {formatRelativeTime(entry.createdAt)}
                  </td>
                  <td className="w-56 py-1.5 pr-3">
                    <div className="flex justify-end gap-1 opacity-0 group-hover:opacity-100">
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        title="重发"
                        onClick={() => void handleResend(entry)}
                      >
                        <RotateCcwIcon />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        title="保存到当前集合"
                        onClick={() => void handleSaveToCollection(entry)}
                      >
                        <SaveIcon />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        title="删除"
                        onClick={() => deleteMutation.mutate(entry.id)}
                      >
                        <Trash2Icon />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <Dialog open={!!detailId} onOpenChange={(open) => (!open ? setDetailId(null) : null)}>
        <DialogContent className="sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle>{detail?.name ?? "历史详情"}</DialogTitle>
            <DialogDescription>
              {detail?.method} {detail?.url}
            </DialogDescription>
          </DialogHeader>
          {detail ? (
            <div className="flex flex-col gap-3">
              <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                <span>状态 {detail.statusCode ?? "-"}</span>
                <span>耗时 {detail.durationMs != null ? formatDuration(detail.durationMs) : "-"}</span>
                <span>响应体 {formatBytes(Number(detail.responseSnapshot?.bodySize ?? 0))}</span>
                {detail.responseBodyFilePath ? <span>大响应体已落盘</span> : null}
              </div>
              <div className="flex flex-col gap-1">
                <p className="text-xs font-medium text-muted-foreground">请求快照</p>
                <CodeEditor
                  value={JSON.stringify(detail.requestSnapshot, null, 2)}
                  language="json"
                  readOnly
                  minHeight={180}
                />
              </div>
              <div className="flex flex-col gap-1">
                <p className="text-xs font-medium text-muted-foreground">响应</p>
                <CodeEditor
                  value={
                    detail.responseBody ??
                    JSON.stringify(detail.responseSnapshot ?? {}, null, 2)
                  }
                  language={detail.responseBody ? "plaintext" : "json"}
                  readOnly
                  minHeight={180}
                />
              </div>
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </div>
  );
}
