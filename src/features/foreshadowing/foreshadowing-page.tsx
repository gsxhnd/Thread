import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { Plus, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  chapterTree,
  foreshadowingCreate,
  foreshadowingDelete,
  foreshadowingLink,
  foreshadowingList,
} from "@/lib/api";
import { ApiError } from "@/lib/api/client";
import { cn } from "@/lib/utils";

export function ForeshadowingPage() {
  const { projectId } = useParams({
    from: "/project/$projectId/foreshadowing",
  });
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [filter, setFilter] = useState<"all" | "planted" | "resolved">("all");

  const { data = [] } = useQuery({
    queryKey: ["foreshadowings", projectId, filter],
    queryFn: () =>
      foreshadowingList(projectId, filter === "all" ? undefined : filter),
  });
  const { data: tree = [] } = useQuery({
    queryKey: ["chapterTree", projectId],
    queryFn: () => chapterTree(projectId),
  });

  const chapters = useMemo(
    () => tree.flatMap((v) => v.chapters.map((c) => ({ ...c, volume: v.title }))),
    [tree],
  );

  const invalidate = () => {
    void qc.invalidateQueries({ queryKey: ["foreshadowings", projectId] });
    void qc.invalidateQueries({ queryKey: ["foreshadowUnresolved", projectId] });
  };

  const createMut = useMutation({
    mutationFn: () =>
      foreshadowingCreate({
        projectId,
        title,
        description: description || undefined,
        importance: 1,
      }),
    onSuccess: () => {
      setTitle("");
      setDescription("");
      invalidate();
      toast.success("伏笔已创建");
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "失败"),
  });

  const linkMut = useMutation({
    mutationFn: (input: {
      id: string;
      setupChapterId?: string | null;
      payoffChapterId?: string | null;
    }) => foreshadowingLink(input),
    onSuccess: () => {
      invalidate();
      toast.success("关联已更新");
    },
  });

  const delMut = useMutation({
    mutationFn: (id: string) => foreshadowingDelete(id),
    onSuccess: invalidate,
  });

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-border px-6 py-4">
        <div>
          <h1 className="text-lg font-semibold">伏笔 · 灰线</h1>
          <p className="text-sm text-muted-foreground">
            setup → payoff，追踪未回收线索
          </p>
        </div>
        <div className="flex gap-1">
          {(
            [
              ["all", "全部"],
              ["planted", "未回收"],
              ["resolved", "已回收"],
            ] as const
          ).map(([k, label]) => (
            <button
              key={k}
              type="button"
              className={cn(
                "rounded-md px-3 py-1.5 text-sm",
                filter === k ? "bg-accent font-medium" : "text-muted-foreground",
              )}
              onClick={() => setFilter(k)}
            >
              {label}
            </button>
          ))}
        </div>
      </header>
      <div className="grid flex-1 gap-6 overflow-auto p-6 lg:grid-cols-[320px_1fr]">
        <div className="flex h-fit flex-col gap-2 rounded-lg border border-border p-4">
          <Input
            placeholder="伏笔标题"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <Textarea
            placeholder="描述"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
          />
          <Button disabled={!title.trim()} onClick={() => createMut.mutate()}>
            <Plus className="size-4" />
            埋下伏笔
          </Button>
        </div>
        <ul className="space-y-3">
          {data.map((f) => (
            <li
              key={f.id}
              className={cn(
                "rounded-lg border p-4",
                f.status === "planted"
                  ? "border-amber-500/40 bg-amber-500/5"
                  : "border-border",
              )}
            >
              <div className="flex items-start justify-between gap-2">
                <div>
                  <div className="font-medium">{f.title}</div>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {f.description || "暂无描述"}
                  </p>
                  <div className="mt-1 text-xs text-muted-foreground">
                    状态：{f.status === "resolved" ? "已回收" : "未回收"}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => delMut.mutate(f.id)}
                >
                  <Trash2 className="size-4" />
                </Button>
              </div>
              <div className="mt-3 grid gap-2 sm:grid-cols-2">
                <label className="text-xs text-muted-foreground">
                  Setup（埋设）
                  <select
                    className="mt-1 h-9 w-full rounded-md border border-input bg-transparent px-2 text-sm"
                    value={f.setupChapterId ?? ""}
                    onChange={(e) =>
                      linkMut.mutate({
                        id: f.id,
                        setupChapterId: e.target.value || null,
                      })
                    }
                  >
                    <option value="">未指定</option>
                    {chapters.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.volume} / {c.title}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="text-xs text-muted-foreground">
                  Payoff（呼应）
                  <select
                    className="mt-1 h-9 w-full rounded-md border border-input bg-transparent px-2 text-sm"
                    value={f.payoffChapterId ?? ""}
                    onChange={(e) =>
                      linkMut.mutate({
                        id: f.id,
                        payoffChapterId: e.target.value || null,
                      })
                    }
                  >
                    <option value="">未指定</option>
                    {chapters.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.volume} / {c.title}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            </li>
          ))}
          {data.length === 0 && (
            <p className="text-sm text-muted-foreground">暂无伏笔</p>
          )}
        </ul>
      </div>
    </div>
  );
}
