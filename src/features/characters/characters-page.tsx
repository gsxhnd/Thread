import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  characterCreate,
  characterDelete,
  characterList,
  relationCreate,
  relationGraph,
} from "@/lib/api";
import { ApiError } from "@/lib/api/client";

export function CharactersPage() {
  const { projectId } = useParams({ from: "/project/$projectId/characters" });
  const qc = useQueryClient();
  const [name, setName] = useState("");
  const [bio, setBio] = useState("");
  const [fromId, setFromId] = useState("");
  const [toId, setToId] = useState("");
  const [relType, setRelType] = useState("");

  const { data: characters = [] } = useQuery({
    queryKey: ["characters", projectId],
    queryFn: () => characterList(projectId),
  });
  const { data: graph } = useQuery({
    queryKey: ["relationGraph", projectId],
    queryFn: () => relationGraph(projectId),
  });

  const invalidate = () => {
    void qc.invalidateQueries({ queryKey: ["characters", projectId] });
    void qc.invalidateQueries({ queryKey: ["relationGraph", projectId] });
  };

  const createMut = useMutation({
    mutationFn: () => characterCreate({ projectId, name, bio: bio || undefined }),
    onSuccess: () => {
      setName("");
      setBio("");
      invalidate();
      toast.success("人物已创建");
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "失败"),
  });

  const relMut = useMutation({
    mutationFn: () =>
      relationCreate({
        projectId,
        fromCharId: fromId,
        toCharId: toId,
        relationType: relType,
      }),
    onSuccess: () => {
      setRelType("");
      invalidate();
      toast.success("关系已添加");
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : "失败"),
  });

  const delMut = useMutation({
    mutationFn: (id: string) => characterDelete(id),
    onSuccess: invalidate,
  });

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-6 py-4">
        <h1 className="text-lg font-semibold">人物设定</h1>
        <p className="text-sm text-muted-foreground">人物卡与关系图</p>
      </header>
      <div className="grid flex-1 gap-6 overflow-auto p-6 lg:grid-cols-2">
        <section className="space-y-4">
          <div className="flex flex-col gap-2 rounded-lg border border-border p-4">
            <Input
              placeholder="姓名"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <Textarea
              placeholder="简介"
              value={bio}
              onChange={(e) => setBio(e.target.value)}
              rows={3}
            />
            <Button
              disabled={!name.trim()}
              onClick={() => createMut.mutate()}
            >
              <Plus className="size-4" />
              添加人物
            </Button>
          </div>
          <ul className="space-y-2">
            {characters.map((c) => (
              <li
                key={c.id}
                className="flex items-start justify-between rounded-lg border border-border p-3"
              >
                <div>
                  <div className="font-medium">{c.name}</div>
                  {c.alias && (
                    <div className="text-xs text-muted-foreground">{c.alias}</div>
                  )}
                  <p className="mt-1 text-sm text-muted-foreground">
                    {c.bio || "暂无简介"}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => delMut.mutate(c.id)}
                >
                  <Trash2 className="size-4" />
                </Button>
              </li>
            ))}
          </ul>
        </section>

        <section className="space-y-4">
          <h2 className="text-sm font-semibold">关系</h2>
          <div className="flex flex-col gap-2 rounded-lg border border-border p-4">
            <select
              className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
              value={fromId}
              onChange={(e) => setFromId(e.target.value)}
            >
              <option value="">从…</option>
              {characters.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
            <Input
              placeholder="关系类型（父子/师徒/敌对…）"
              value={relType}
              onChange={(e) => setRelType(e.target.value)}
            />
            <select
              className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
              value={toId}
              onChange={(e) => setToId(e.target.value)}
            >
              <option value="">到…</option>
              {characters.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
            <Button
              disabled={!fromId || !toId || !relType.trim()}
              onClick={() => relMut.mutate()}
            >
              添加关系
            </Button>
          </div>
          <ul className="space-y-2">
            {(graph?.edges ?? []).map((e) => {
              const from = graph?.nodes.find((n) => n.id === e.fromCharId)?.name;
              const to = graph?.nodes.find((n) => n.id === e.toCharId)?.name;
              return (
                <li
                  key={e.id}
                  className="rounded-md border border-border px-3 py-2 text-sm"
                >
                  <span className="font-medium">{from}</span>
                  <span className="mx-2 text-muted-foreground">
                    —{e.relationType}→
                  </span>
                  <span className="font-medium">{to}</span>
                </li>
              );
            })}
          </ul>
        </section>
      </div>
    </div>
  );
}
