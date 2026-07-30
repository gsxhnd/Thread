import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { BookOpen, Plus } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { WindowTitleBar } from "@/components/window-title-bar";
import { projectCreate, projectList } from "@/lib/api";
import { ApiError } from "@/lib/api/client";
import { getPlatform } from "@/lib/platform";

export function BookshelfPage() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [synopsis, setSynopsis] = useState("");

  const { data: projects = [], isLoading } = useQuery({
    queryKey: ["projects"],
    queryFn: projectList,
  });

  const createMut = useMutation({
    mutationFn: () => projectCreate({ title, synopsis: synopsis || undefined }),
    onSuccess: (project) => {
      void qc.invalidateQueries({ queryKey: ["projects"] });
      setOpen(false);
      setTitle("");
      setSynopsis("");
      toast.success("作品已创建");
      void navigate({
        to: "/project/$projectId/write",
        params: { projectId: project.id },
      });
    },
    onError: (e) => {
      toast.error(e instanceof ApiError ? e.message : "创建失败");
    },
  });

  return (
    <div
      data-app-shell
      data-platform={getPlatform()}
      className="flex h-screen flex-col bg-shell-main text-foreground"
    >
      <WindowTitleBar
        className="h-auto items-start py-5 pr-8 pl-[max(2rem,var(--titlebar-macos-inset))]"
        trailing={
          <Button onClick={() => setOpen(true)}>
            <Plus className="size-4" />
            新建作品
          </Button>
        }
      >
        <div className="py-0.5">
          <h1 className="text-2xl font-semibold tracking-tight">
            Thread · 灰线
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            草蛇灰线，伏脉千里
          </p>
        </div>
      </WindowTitleBar>

      <main className="flex-1 overflow-auto p-8">
        {isLoading ? (
          <p className="text-muted-foreground">加载中…</p>
        ) : projects.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-4 text-center">
            <BookOpen className="size-12 text-muted-foreground/60" />
            <div>
              <p className="text-lg font-medium">还没有作品</p>
              <p className="mt-1 text-sm text-muted-foreground">
                创建第一部作品，开始埋下你的灰线
              </p>
            </div>
            <Button onClick={() => setOpen(true)}>
              <Plus className="size-4" />
              新建作品
            </Button>
          </div>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            {projects.map((p) => (
              <Link
                key={p.id}
                to="/project/$projectId/write"
                params={{ projectId: p.id }}
                className="group rounded-xl border border-border bg-card/40 p-5 transition hover:border-primary/40 hover:bg-card"
              >
                <div className="mb-3 flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
                  <BookOpen className="size-5" />
                </div>
                <h2 className="truncate text-base font-semibold group-hover:text-primary">
                  {p.title}
                </h2>
                <p className="mt-2 line-clamp-2 text-sm text-muted-foreground">
                  {p.synopsis || "暂无简介"}
                </p>
              </Link>
            ))}
          </div>
        )}
      </main>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>新建作品</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-3 py-2">
            <Input
              placeholder="作品标题"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              autoFocus
            />
            <Textarea
              placeholder="简介（可选）"
              value={synopsis}
              onChange={(e) => setSynopsis(e.target.value)}
              rows={3}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              取消
            </Button>
            <Button
              disabled={!title.trim() || createMut.isPending}
              onClick={() => createMut.mutate()}
            >
              创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
