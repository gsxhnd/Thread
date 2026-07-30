import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { EditorContent, type Editor } from "@tiptap/react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Focus,
  Plus,
  Save,
} from "lucide-react";
import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { AiPanel } from "@/components/ai-panel";
import { ForeshadowSidePanel } from "@/components/foreshadow-side-panel";
import {
  chapterCreate,
  chapterGet,
  chapterSave,
  chapterTree,
  foreshadowingUnresolved,
  onChapterSaved,
  volumeCreate,
} from "@/lib/api";
import { ApiError } from "@/lib/api/client";
import { useNovelEditor } from "@/lib/editor";
import { cn } from "@/lib/utils";
import { useEditorStore } from "@/stores/editor-store";
import { useUiStore } from "@/stores/ui-store";

const AUTOSAVE_MS = 800;

type SavePayload = { id: string; content: string; title: string };

function editorMarkdown(editor: Editor | null, fallback: string): string {
  if (!editor) return fallback;
  return editor.storage.markdown.getMarkdown() as string;
}

export function WritePage() {
  const { projectId } = useParams({ from: "/project/$projectId" });
  const params = useParams({ strict: false }) as { chapterId?: string };
  const chapterId = params.chapterId;
  const navigate = useNavigate();
  const qc = useQueryClient();

  const {
    leftCollapsed,
    rightCollapsed,
    rightTab,
    focusMode,
    collapsedVolumeIds,
    setLeftCollapsed,
    setRightCollapsed,
    setRightTab,
    toggleFocusMode,
    toggleVolumeCollapsed,
  } = useUiStore();

  const {
    chapterId: activeId,
    draft,
    title,
    dirty,
    saveState,
    wordCount,
    setChapter,
    setDraft,
    setTitle,
    setSaving,
    markSaved,
    markError,
  } = useEditorStore();

  const treeQuery = useQuery({
    queryKey: ["chapterTree", projectId],
    queryFn: () => chapterTree(projectId),
  });

  const unresolvedQuery = useQuery({
    queryKey: ["foreshadowUnresolved", projectId],
    queryFn: () => foreshadowingUnresolved(projectId),
  });

  const chapterQuery = useQuery({
    queryKey: ["chapter", chapterId],
    queryFn: () => chapterGet(chapterId!),
    enabled: !!chapterId,
  });

  const editor = useNovelEditor(chapterQuery.data?.content ?? "", (md) => {
    setDraft(md);
  });
  const editorRef = useRef(editor);
  editorRef.current = editor;

  const saveMut = useMutation({
    mutationFn: async (payload: SavePayload) => {
      setSaving();
      return chapterSave(payload);
    },
    onSuccess: (res, vars) => {
      markSaved(res.wordCount, vars.id);
      void qc.invalidateQueries({ queryKey: ["chapterTree", projectId] });
      void qc.invalidateQueries({ queryKey: ["chapter", vars.id] });
    },
    onError: (e) => {
      markError();
      toast.error(e instanceof ApiError ? e.message : "保存失败");
    },
  });

  const saveMutRef = useRef(saveMut.mutate);
  saveMutRef.current = saveMut.mutate;

  const flushActiveIfDirty = (idHint?: string) => {
    const state = useEditorStore.getState();
    const id = idHint ?? state.chapterId;
    if (!id || !state.dirty) return;
    if (state.chapterId !== id) return;
    saveMutRef.current({
      id,
      content: editorMarkdown(editorRef.current, state.draft),
      title: state.title,
    });
  };

  // Open first chapter if none selected
  useEffect(() => {
    if (chapterId || !treeQuery.data?.length) return;
    const first = treeQuery.data[0]?.chapters[0];
    if (first) {
      void navigate({
        to: "/project/$projectId/write/$chapterId",
        params: { projectId, chapterId: first.id },
        replace: true,
      });
    }
  }, [chapterId, treeQuery.data, navigate, projectId]);

  const prevChapterRef = useRef<string | undefined>(undefined);

  // Flush previous chapter BEFORE loading the next (must run before setChapter).
  useEffect(() => {
    const prev = prevChapterRef.current;
    if (prev && prev !== chapterId) {
      flushActiveIfDirty(prev);
    }
    prevChapterRef.current = chapterId;
  }, [chapterId]);

  useEffect(() => {
    const data = chapterQuery.data;
    if (!data || data.id !== chapterId) return;
    const current = useEditorStore.getState();
    // Don't clobber unsaved edits for the already-active chapter.
    if (current.chapterId === data.id && current.dirty) return;
    if (
      current.chapterId === data.id &&
      current.title === data.title &&
      current.draft === data.content
    ) {
      return;
    }
    setChapter(data.id, data.title, data.content, data.wordCount);
  }, [chapterQuery.data, chapterId, setChapter]);

  // Sync editor when chapter switches
  useEffect(() => {
    if (!editor || !chapterQuery.data) return;
    if (chapterQuery.data.id !== chapterId) return;
    editor.commands.setContent(chapterQuery.data.content);
  }, [editor, chapterQuery.data?.id, chapterId]);

  // Autosave debounce
  const timer = useRef<number | null>(null);
  useEffect(() => {
    if (!dirty || !activeId) return;
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      flushActiveIfDirty(activeId);
    }, AUTOSAVE_MS);
    return () => {
      if (timer.current) window.clearTimeout(timer.current);
    };
  }, [dirty, draft, title, activeId]);

  // Flush on blur / unmount
  useEffect(() => {
    const flush = () => flushActiveIfDirty();
    window.addEventListener("blur", flush);
    return () => {
      window.removeEventListener("blur", flush);
      flush();
    };
  }, []);

  useEffect(() => {
    return onChapterSaved((p) => {
      markSaved(p.wordCount, p.chapterId);
    });
  }, [markSaved]);

  const addVolume = useMutation({
    mutationFn: () => volumeCreate({ projectId, title: "新卷" }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["chapterTree", projectId] });
    },
    onError: (e) =>
      toast.error(e instanceof ApiError ? e.message : "创建卷失败"),
  });

  const addChapter = useMutation({
    mutationFn: (volumeId: string) =>
      chapterCreate({ volumeId, title: "新章节" }),
    onSuccess: (ch) => {
      void qc.invalidateQueries({ queryKey: ["chapterTree", projectId] });
      void navigate({
        to: "/project/$projectId/write/$chapterId",
        params: { projectId, chapterId: ch.id },
      });
    },
    onError: (e) =>
      toast.error(e instanceof ApiError ? e.message : "创建章节失败"),
  });

  const saveLabel =
    saveState === "saving"
      ? "保存中…"
      : saveState === "saved"
        ? "已保存"
        : saveState === "error"
          ? "保存失败"
          : dirty
            ? "未保存"
            : "已保存";

  const liveWordCount =
    editor?.storage.characterCount?.characters?.() ?? wordCount;

  const saveNow = () => {
    const state = useEditorStore.getState();
    if (!state.chapterId || !state.dirty) return;
    saveMut.mutate({
      id: state.chapterId,
      content: editorMarkdown(editor, state.draft),
      title: state.title,
    });
  };

  return (
    <div className="flex h-full">
      {!leftCollapsed && (
        <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-shell-secondary">
          <div className="flex items-center justify-between border-b border-border px-3 py-2">
            <span className="text-xs font-medium text-muted-foreground">
              章节
            </span>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2"
              onClick={() => addVolume.mutate()}
            >
              <Plus className="size-3.5" />
              卷
            </Button>
          </div>
          <ScrollArea className="flex-1">
            <div className="space-y-3 p-2">
              {(treeQuery.data ?? []).map((vol) => {
                const volumeCollapsed = !!collapsedVolumeIds[vol.id];
                return (
                  <div key={vol.id}>
                    <div className="mb-1 flex items-center gap-0.5 px-1">
                      <button
                        type="button"
                        className="flex min-w-0 flex-1 items-center gap-1 rounded-md px-1 py-0.5 text-left hover:bg-accent"
                        aria-expanded={!volumeCollapsed}
                        onClick={() => toggleVolumeCollapsed(vol.id)}
                      >
                        {volumeCollapsed ? (
                          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
                        ) : (
                          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
                        )}
                        <span className="truncate text-xs font-semibold">
                          {vol.title}
                        </span>
                      </button>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-6 w-6 shrink-0 p-0"
                        title="新建章节"
                        onClick={() => addChapter.mutate(vol.id)}
                      >
                        <Plus className="size-3" />
                      </Button>
                    </div>
                    {!volumeCollapsed && (
                      <div className="space-y-0.5">
                        {vol.chapters.map((ch) => (
                          <Link
                            key={ch.id}
                            to="/project/$projectId/write/$chapterId"
                            params={{ projectId, chapterId: ch.id }}
                            className={cn(
                              "block rounded-md px-2 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                              ch.id === chapterId &&
                                "bg-accent font-medium text-accent-foreground",
                            )}
                          >
                            <div className="truncate">{ch.title}</div>
                            <div className="text-[10px] opacity-70">
                              {ch.wordCount} 字
                            </div>
                          </Link>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </ScrollArea>
        </aside>
      )}

      <main className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-3 py-2">
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0"
            onClick={() => setLeftCollapsed(!leftCollapsed)}
          >
            {leftCollapsed ? (
              <ChevronRight className="size-4" />
            ) : (
              <ChevronLeft className="size-4" />
            )}
          </Button>
          <Input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            className="h-8 max-w-xs border-none bg-transparent px-1 text-base font-medium shadow-none focus-visible:ring-0"
            placeholder="章节标题"
            disabled={!activeId}
          />
          <div className="ml-auto flex items-center gap-2">
            <span className="text-xs text-muted-foreground">{saveLabel}</span>
            <Button
              size="sm"
              variant="ghost"
              className="h-7"
              disabled={!dirty || saveMut.isPending}
              onClick={saveNow}
            >
              <Save className="size-3.5" />
              保存
            </Button>
            <Button
              size="sm"
              variant={focusMode ? "default" : "ghost"}
              className="h-7"
              onClick={toggleFocusMode}
            >
              <Focus className="size-3.5" />
              沉浸
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 w-7 p-0"
              onClick={() => setRightCollapsed(!rightCollapsed)}
            >
              {rightCollapsed ? (
                <ChevronLeft className="size-4" />
              ) : (
                <ChevronRight className="size-4" />
              )}
            </Button>
          </div>
        </div>

        <ScrollArea className="flex-1">
          {activeId && editor ? (
            <EditorContent editor={editor} />
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              选择或创建章节开始写作
            </div>
          )}
        </ScrollArea>

        <footer className="flex items-center gap-4 border-t border-border px-4 py-1.5 text-xs text-muted-foreground">
          <span>{liveWordCount} 字</span>
          {(unresolvedQuery.data?.length ?? 0) > 0 && (
            <button
              type="button"
              className="text-amber-600 hover:underline dark:text-amber-400"
              onClick={() => {
                setRightCollapsed(false);
                setRightTab("foreshadowing");
              }}
            >
              未回收伏笔 {unresolvedQuery.data?.length}
            </button>
          )}
        </footer>
      </main>

      {!rightCollapsed && (
        <aside className="flex w-72 shrink-0 flex-col border-l border-border bg-shell-secondary">
          <div className="flex border-b border-border">
            {(
              [
                ["foreshadowing", "伏笔"],
                ["ai", "AI"],
                ["context", "设定"],
              ] as const
            ).map(([key, label]) => (
              <button
                key={key}
                type="button"
                className={cn(
                  "flex-1 py-2 text-xs",
                  rightTab === key
                    ? "border-b-2 border-primary font-medium text-foreground"
                    : "text-muted-foreground",
                )}
                onClick={() => setRightTab(key)}
              >
                {label}
                {key === "foreshadowing" &&
                  (unresolvedQuery.data?.length ?? 0) > 0 && (
                    <span className="ml-1 rounded-full bg-amber-500/20 px-1.5 text-[10px] text-amber-700 dark:text-amber-300">
                      {unresolvedQuery.data?.length}
                    </span>
                  )}
              </button>
            ))}
          </div>
          <div className="min-h-0 flex-1 overflow-auto p-3">
            {rightTab === "context" && (
              <p className="text-xs text-muted-foreground">
                在人物 / 世界观页面管理设定后，可在此查看当前章节相关上下文。前往左侧导航完善
                story bible。
              </p>
            )}
            {rightTab === "foreshadowing" && (
              <ForeshadowSidePanel projectId={projectId} />
            )}
            {rightTab === "ai" && (
              <AiPanel
                getSelection={() =>
                  editor?.state.doc.textBetween(
                    editor.state.selection.from,
                    editor.state.selection.to,
                    "\n",
                  ) ?? ""
                }
                getContext={() =>
                  editor
                    ? (editor.storage.markdown.getMarkdown() as string).slice(
                        0,
                        4000,
                      )
                    : draft.slice(0, 4000)
                }
                onApply={(text) => {
                  if (!editor) return;
                  editor.commands.insertContent(text);
                  setDraft(editor.storage.markdown.getMarkdown() as string);
                }}
              />
            )}
          </div>
        </aside>
      )}
    </div>
  );
}
