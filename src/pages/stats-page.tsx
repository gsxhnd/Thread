import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  goalGet,
  goalSet,
  statsCalendar,
  statsOverview,
  statsStreak,
} from "@/lib/api";

export function StatsPage() {
  const { projectId } = useParams({ from: "/project/$projectId/stats" });
  const qc = useQueryClient();
  const year = new Date().getFullYear();
  const [daily, setDaily] = useState("");
  const [total, setTotal] = useState("");

  const { data: overview } = useQuery({
    queryKey: ["statsOverview", projectId],
    queryFn: () => statsOverview(projectId),
  });
  const { data: streak } = useQuery({
    queryKey: ["statsStreak", projectId],
    queryFn: () => statsStreak(projectId),
  });
  const { data: calendar = [] } = useQuery({
    queryKey: ["statsCalendar", projectId, year],
    queryFn: () => statsCalendar(projectId, year),
  });
  const { data: goal } = useQuery({
    queryKey: ["goal", projectId],
    queryFn: () => goalGet(projectId),
  });

  const goalMut = useMutation({
    mutationFn: () => {
      const dailyWords = daily ? Number(daily) : goal?.dailyWords;
      const totalWords = total ? Number(total) : goal?.totalWords;
      if (daily && !Number.isFinite(dailyWords)) {
        throw new Error("日更目标必须是数字");
      }
      if (total && !Number.isFinite(totalWords)) {
        throw new Error("总量目标必须是数字");
      }
      return goalSet({
        projectId,
        dailyWords: dailyWords ?? undefined,
        totalWords: totalWords ?? undefined,
      });
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["goal", projectId] });
      setDaily("");
      setTotal("");
      toast.success("目标已保存");
    },
    onError: (e) =>
      toast.error(e instanceof Error ? e.message : "保存目标失败"),
  });

  const maxWords = Math.max(1, ...calendar.map((d) => d.words));

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-6 py-4">
        <h1 className="text-lg font-semibold">写作统计</h1>
        <p className="text-sm text-muted-foreground">字数、目标与连续打卡</p>
      </header>
      <div className="flex-1 space-y-6 overflow-auto p-6">
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {[
            ["总字数", overview?.totalWords ?? 0],
            ["今日新增", overview?.wordsToday ?? 0],
            ["章节数", overview?.chapterCount ?? 0],
            ["未回收伏笔", overview?.unresolvedForeshadowings ?? 0],
          ].map(([label, value]) => (
            <div
              key={String(label)}
              className="rounded-lg border border-border p-4"
            >
              <div className="text-xs text-muted-foreground">{label}</div>
              <div className="mt-1 text-2xl font-semibold tabular-nums">
                {value}
              </div>
            </div>
          ))}
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div className="rounded-lg border border-border p-4">
            <div className="text-sm font-medium">连续打卡</div>
            <p className="mt-2 text-3xl font-semibold tabular-nums">
              {streak?.current ?? 0}
              <span className="ml-2 text-sm font-normal text-muted-foreground">
                天（最长 {streak?.longest ?? 0}）
              </span>
            </p>
          </div>
          <div className="space-y-2 rounded-lg border border-border p-4">
            <div className="text-sm font-medium">写作目标</div>
            <div className="flex gap-2">
              <Input
                placeholder={`日更 ${goal?.dailyWords ?? "—"}`}
                value={daily}
                onChange={(e) => setDaily(e.target.value)}
              />
              <Input
                placeholder={`总量 ${goal?.totalWords ?? "—"}`}
                value={total}
                onChange={(e) => setTotal(e.target.value)}
              />
              <Button onClick={() => goalMut.mutate()}>保存</Button>
            </div>
          </div>
        </div>

        <div className="rounded-lg border border-border p-4">
          <div className="mb-3 text-sm font-medium">{year} 写作日历</div>
          <div className="flex flex-wrap gap-1">
            {calendar.length === 0 ? (
              <p className="text-sm text-muted-foreground">暂无记录</p>
            ) : (
              calendar.map((d) => (
                <div
                  key={d.date}
                  title={`${d.date}: ${d.words} 字`}
                  className="size-3 rounded-sm"
                  style={{
                    backgroundColor: `color-mix(in oklch, var(--primary) ${Math.round(
                      (d.words / maxWords) * 100,
                    )}%, var(--muted))`,
                  }}
                />
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
