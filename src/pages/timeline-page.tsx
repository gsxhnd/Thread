import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { eventCreate, eventDelete, timelineList } from "@/lib/api";
import { ApiError } from "@/lib/api/client";

export function TimelinePage() {
  const { projectId } = useParams({ from: "/project/$projectId/timeline" });
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [storyTime, setStoryTime] = useState("");
  const [description, setDescription] = useState("");
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);

  const { data = [] } = useQuery({
    queryKey: ["timeline", projectId],
    queryFn: () => timelineList(projectId),
  });

  const createMut = useMutation({
    mutationFn: () =>
      eventCreate({
        projectId,
        title,
        storyTime: storyTime || undefined,
        description: description || undefined,
      }),
    onSuccess: () => {
      setTitle("");
      setStoryTime("");
      setDescription("");
      void qc.invalidateQueries({ queryKey: ["timeline", projectId] });
      toast.success("事件已添加");
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "失败"),
  });

  const delMut = useMutation({
    mutationFn: (id: string) => eventDelete(id),
    onSuccess: () =>
      void qc.invalidateQueries({ queryKey: ["timeline", projectId] }),
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "删除失败"),
  });

  const pendingTitle =
    data.find((ev) => ev.id === pendingDeleteId)?.title ?? "该事件";

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-6 py-4">
        <h1 className="text-lg font-semibold">时间线</h1>
        <p className="text-sm text-muted-foreground">故事内事件轴</p>
      </header>
      <div className="grid flex-1 gap-6 overflow-auto p-6 lg:grid-cols-[320px_1fr]">
        <div className="flex h-fit flex-col gap-2 rounded-lg border border-border p-4">
          <Input
            placeholder="事件标题"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <Input
            placeholder="故事时间（如：元武三年·春）"
            value={storyTime}
            onChange={(e) => setStoryTime(e.target.value)}
          />
          <Textarea
            placeholder="描述"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
          />
          <Button disabled={!title.trim()} onClick={() => createMut.mutate()}>
            <Plus className="size-4" />
            添加事件
          </Button>
        </div>
        <ol className="relative space-y-4 border-l border-border pl-6">
          {data.map((ev) => (
            <li key={ev.id} className="relative">
              <span className="absolute -left-[1.6rem] top-1.5 size-2.5 rounded-full bg-primary" />
              <div className="flex items-start justify-between rounded-lg border border-border p-3">
                <div>
                  {ev.storyTime && (
                    <div className="text-xs text-primary">{ev.storyTime}</div>
                  )}
                  <div className="font-medium">{ev.title}</div>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {ev.description || "暂无描述"}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setPendingDeleteId(ev.id)}
                >
                  <Trash2 className="size-4" />
                </Button>
              </div>
            </li>
          ))}
          {data.length === 0 && (
            <p className="text-sm text-muted-foreground">暂无事件</p>
          )}
        </ol>
      </div>

      <ConfirmDialog
        open={pendingDeleteId !== null}
        title="删除事件"
        description={`确定删除「${pendingTitle}」？此操作不可撤销。`}
        destructive
        confirmLabel="删除"
        onConfirm={() => {
          if (pendingDeleteId) delMut.mutate(pendingDeleteId);
        }}
        onOpenChange={(open) => {
          if (!open) setPendingDeleteId(null);
        }}
      />
    </div>
  );
}
