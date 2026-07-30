import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  factionCreate,
  factionDelete,
  itemCreate,
  itemDelete,
  locationCreate,
  locationDelete,
  worldbuildingList,
} from "@/lib/api";
import { ApiError } from "@/lib/api/client";
import { cn } from "@/lib/utils";

type EntryKind = "location" | "faction" | "item";

export function WorldPage() {
  const { projectId } = useParams({ from: "/project/$projectId/world" });
  const qc = useQueryClient();
  const [kind, setKind] = useState<EntryKind>("location");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);

  const { data = [] } = useQuery({
    queryKey: ["world", projectId, kind],
    queryFn: () => worldbuildingList(projectId, kind),
  });

  const invalidate = () =>
    void qc.invalidateQueries({ queryKey: ["world", projectId] });

  const createMut = useMutation({
    mutationFn: async () => {
      if (kind === "location") {
        return locationCreate({
          projectId,
          name,
          description: description || undefined,
        });
      }
      if (kind === "faction") {
        return factionCreate({
          projectId,
          name,
          description: description || undefined,
        });
      }
      return itemCreate({
        projectId,
        name,
        description: description || undefined,
      });
    },
    onSuccess: () => {
      setName("");
      setDescription("");
      invalidate();
      toast.success("词条已创建");
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "失败"),
  });

  const delMut = useMutation({
    mutationFn: async (id: string) => {
      if (kind === "location") return locationDelete(id);
      if (kind === "faction") return factionDelete(id);
      return itemDelete(id);
    },
    onSuccess: invalidate,
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "删除失败"),
  });

  const pendingName = data.find((e) => e.id === pendingDeleteId)?.name ?? "该词条";

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-6 py-4">
        <h1 className="text-lg font-semibold">世界观百科</h1>
        <p className="text-sm text-muted-foreground">地点 · 势力 · 物品</p>
      </header>
      <div className="flex gap-2 border-b border-border px-6 py-2">
        {(
          [
            ["location", "地点"],
            ["faction", "势力"],
            ["item", "物品"],
          ] as const
        ).map(([k, label]) => (
          <button
            key={k}
            type="button"
            className={cn(
              "rounded-md px-3 py-1.5 text-sm",
              kind === k
                ? "bg-accent font-medium"
                : "text-muted-foreground hover:bg-muted",
            )}
            onClick={() => setKind(k)}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="grid flex-1 gap-6 overflow-auto p-6 lg:grid-cols-[320px_1fr]">
        <div className="flex h-fit flex-col gap-2 rounded-lg border border-border p-4">
          <Input
            placeholder="名称"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Textarea
            placeholder="描述（Markdown）"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={4}
          />
          <Button disabled={!name.trim()} onClick={() => createMut.mutate()}>
            <Plus className="size-4" />
            添加
          </Button>
        </div>
        <ul className="space-y-2">
          {data.map((e) => (
            <li
              key={e.id}
              className="flex items-start justify-between rounded-lg border border-border p-3"
            >
              <div>
                <div className="font-medium">{e.name}</div>
                <p className="mt-1 text-sm text-muted-foreground">
                  {e.description || "暂无描述"}
                </p>
              </div>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setPendingDeleteId(e.id)}
              >
                <Trash2 className="size-4" />
              </Button>
            </li>
          ))}
          {data.length === 0 && (
            <p className="text-sm text-muted-foreground">暂无词条</p>
          )}
        </ul>
      </div>

      <ConfirmDialog
        open={pendingDeleteId !== null}
        title="删除词条"
        description={`确定删除「${pendingName}」？此操作不可撤销。`}
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
