import { useMutation } from "@tanstack/react-query";
import { Loader2, Square, Sparkles } from "lucide-react";
import { useEffect, useId, useRef } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { aiCancel, aiComplete, onAiChunk, onAiDone, onAiError } from "@/lib/api";
import { ApiError } from "@/lib/api/client";
import type { AiMode } from "@/lib/api/types";
import { useAiStore } from "@/stores/ai-store";
import { cn } from "@/lib/utils";

const MODES: { id: AiMode; label: string }[] = [
  { id: "continue", label: "续写" },
  { id: "polish", label: "润色" },
  { id: "inspire", label: "灵感" },
  { id: "ask", label: "问答" },
];

type Props = {
  getSelection: () => string;
  getContext: () => string;
  onApply: (text: string) => void;
};

export function AiPanel({ getSelection, getContext, onApply }: Props) {
  const {
    mode,
    buffer,
    requestId,
    running,
    error,
    setMode,
    start,
    append,
    finish,
    fail,
    clear,
  } = useAiStore();
  const promptId = useId();
  const promptRef = useRef("");

  useEffect(() => {
    const offChunk = onAiChunk((p) => {
      if (p.requestId === useAiStore.getState().requestId) append(p.delta);
    });
    const offDone = onAiDone((p) => {
      if (p.requestId === useAiStore.getState().requestId) finish();
    });
    const offError = onAiError((p) => {
      if (p.requestId === useAiStore.getState().requestId) fail(p.message);
    });
    return () => {
      offChunk();
      offDone();
      offError();
    };
  }, [append, finish, fail]);

  const runMut = useMutation({
    mutationFn: async () => {
      const selection = getSelection();
      const prompt =
        promptRef.current.trim() ||
        selection ||
        (mode === "continue" ? "请续写下文" : "请给出建议");
      const id = crypto.randomUUID();
      start(id);
      return aiComplete({
        mode,
        prompt,
        context: getContext(),
        stream: true,
        requestId: id,
      });
    },
    onError: (e) => {
      const msg =
        e instanceof ApiError
          ? e.code === "AI_NO_KEY"
            ? "请先在设置中配置 AI Key"
            : e.message
          : "AI 请求失败";
      fail(msg);
      toast.error(msg);
    },
  });

  return (
    <div className="flex h-full flex-col gap-3">
      <div className="flex flex-wrap gap-1">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            className={cn(
              "rounded-md px-2 py-1 text-xs",
              mode === m.id
                ? "bg-primary text-primary-foreground"
                : "bg-muted text-muted-foreground",
            )}
            onClick={() => setMode(m.id)}
          >
            {m.label}
          </button>
        ))}
      </div>
      <Textarea
        id={promptId}
        placeholder="指令或问题（可留空，使用选中文本）"
        rows={3}
        className="text-sm"
        onChange={(e) => {
          promptRef.current = e.target.value;
        }}
      />
      <div className="flex gap-2">
        <Button
          size="sm"
          className="flex-1"
          disabled={running}
          onClick={() => runMut.mutate()}
        >
          {running ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Sparkles className="size-3.5" />
          )}
          生成
        </Button>
        {running && requestId && (
          <Button
            size="sm"
            variant="outline"
            onClick={() => void aiCancel(requestId)}
          >
            <Square className="size-3.5" />
          </Button>
        )}
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
      {buffer && (
        <div className="flex-1 space-y-2">
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background/60 p-2 text-xs leading-5">
            {buffer}
          </pre>
          <div className="flex gap-2">
            <Button size="sm" onClick={() => onApply(buffer)}>
              采纳
            </Button>
            <Button size="sm" variant="ghost" onClick={clear}>
              丢弃
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
