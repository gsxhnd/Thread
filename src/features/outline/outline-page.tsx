import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { outlineList, plotlineCreate } from "@/lib/api";
import { ApiError } from "@/lib/api/client";

export function OutlinePage() {
  const { projectId } = useParams({ from: "/project/$projectId/outline" });
  const qc = useQueryClient();
  const [name, setName] = useState("");

  const { data = [] } = useQuery({
    queryKey: ["outline", projectId],
    queryFn: () => outlineList(projectId),
  });

  const createPlot = useMutation({
    mutationFn: () => plotlineCreate({ projectId, name }),
    onSuccess: () => {
      setName("");
      void qc.invalidateQueries({ queryKey: ["outline", projectId] });
      toast.success("剧情线已创建");
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "失败"),
  });

  const byVolume = data.reduce<
    Record<string, { title: string; nodes: typeof data }>
  >((acc, node) => {
    const key = node.volumeId;
    if (!acc[key]) acc[key] = { title: node.volumeTitle, nodes: [] };
    acc[key].nodes.push(node);
    return acc;
  }, {});

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-border px-6 py-4">
        <div>
          <h1 className="text-lg font-semibold">大纲 / 剧情线</h1>
          <p className="text-sm text-muted-foreground">
            按卷查看章节梗概与剧情线关联
          </p>
        </div>
        <div className="flex gap-2">
          <Input
            placeholder="新剧情线名称"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="w-48"
          />
          <Button
            disabled={!name.trim()}
            onClick={() => createPlot.mutate()}
          >
            <Plus className="size-4" />
            剧情线
          </Button>
        </div>
      </header>
      <div className="flex-1 space-y-6 overflow-auto p-6">
        {Object.entries(byVolume).map(([vid, group]) => (
          <section key={vid}>
            <h2 className="mb-3 text-sm font-semibold text-muted-foreground">
              {group.title}
            </h2>
            <div className="space-y-2">
              {group.nodes.map((n) => (
                <div
                  key={n.chapterId}
                  className="rounded-lg border border-border bg-card/40 p-4"
                >
                  <div className="font-medium">{n.chapterTitle}</div>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {n.summary || "暂无梗概（可在写作时补充 summary）"}
                  </p>
                  {n.plotlines.length > 0 && (
                    <div className="mt-2 flex flex-wrap gap-1">
                      {n.plotlines.map((p) => (
                        <span
                          key={p.id}
                          className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] text-primary"
                        >
                          {p.name}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </section>
        ))}
        {data.length === 0 && (
          <p className="text-sm text-muted-foreground">暂无章节</p>
        )}
      </div>
    </div>
  );
}
