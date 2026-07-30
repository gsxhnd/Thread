import { useQuery } from "@tanstack/react-query";
import { foreshadowingUnresolved } from "@/lib/api";

export function ForeshadowSidePanel({ projectId }: { projectId: string }) {
  const { data = [] } = useQuery({
    queryKey: ["foreshadowUnresolved", projectId],
    queryFn: () => foreshadowingUnresolved(projectId),
  });

  if (data.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        暂无未回收伏笔。可在「伏笔」页创建并关联 setup / payoff 章节。
      </p>
    );
  }

  return (
    <ul className="space-y-2">
      {data.map((f) => (
        <li
          key={f.id}
          className="rounded-md border border-amber-500/30 bg-amber-500/5 p-2"
        >
          <div className="text-sm font-medium">{f.title}</div>
          {f.description && (
            <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
              {f.description}
            </p>
          )}
          <div className="mt-1 text-[10px] text-amber-700 dark:text-amber-300">
            重要度 {f.importance} · 未回收
          </div>
        </li>
      ))}
    </ul>
  );
}
