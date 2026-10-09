import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { grpcStreamClose, grpcStreamSend, wsSend } from "@/lib/api";
import { docString, errorMessage, findProtoMethod } from "@/lib/protocol";
import { useSessionStore } from "@/stores/session-store";
import type { RequestTab } from "@/stores/tabs-store";
import { cn } from "@/lib/utils";

export function SessionLog({ tab }: { tab: RequestTab }) {
  const session = useSessionStore((state) => state.byTab[tab.key]);
  const messages = session?.messages ?? [];
  const open = session?.status === "open";
  const method = findProtoMethod(
    session?.services ?? [],
    docString(tab.document, "service"),
    docString(tab.document, "method"),
  );
  const clientStreaming = tab.protocol === "grpc" && !!method?.clientStreaming;
  const showComposer = tab.protocol === "websocket" ? open : clientStreaming && open;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2 text-xs text-muted-foreground">
        <span
          className={cn(
            "size-1.5 rounded-full",
            session?.status === "open"
              ? "bg-status-success"
              : session?.status === "error"
                ? "bg-destructive"
                : "bg-muted-foreground",
          )}
        />
        {statusLabel(session?.status)}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {messages.length === 0 ? (
          <p className="text-xs text-muted-foreground">连接后的收发记录会显示在这里。</p>
        ) : (
          <div className="flex flex-col gap-2">
            {messages.map((message) => (
              <div key={message.id} className="flex flex-col gap-1">
                <span className="text-[10px] tracking-wide text-muted-foreground uppercase">
                  {message.direction === "out" ? "发送" : message.direction === "in" ? "接收" : "状态"}
                </span>
                <pre className="overflow-x-auto rounded-lg bg-muted/60 px-3 py-2 font-mono text-xs whitespace-pre-wrap">
                  {message.text}
                </pre>
              </div>
            ))}
          </div>
        )}
      </div>
      {showComposer ? (
        <Composer
          tabKey={tab.key}
          protocol={tab.protocol}
          connectionId={session?.connectionId}
          clientStreaming={clientStreaming}
        />
      ) : null}
    </div>
  );
}

function Composer({
  tabKey,
  protocol,
  connectionId,
  clientStreaming,
}: {
  tabKey: string;
  protocol: string;
  connectionId?: string;
  clientStreaming: boolean;
}) {
  const [draft, setDraft] = useState("");
  const [binary, setBinary] = useState(false);
  const [pending, setPending] = useState(false);

  const send = async () => {
    if (!connectionId) return;
    setPending(true);
    try {
      if (protocol === "websocket") {
        await wsSend({ connectionId, message: draft, isBinary: binary });
        setDraft("");
        return;
      }
      await grpcStreamSend({ streamId: connectionId, payload: draft || "{}" });
      useSessionStore.getState().pushByTab(tabKey, {
        direction: "out",
        text: draft || "{}",
        at: Date.now(),
      });
      setDraft("");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setPending(false);
    }
  };

  const closeStream = async () => {
    if (!connectionId) return;
    try {
      await grpcStreamClose(connectionId);
      toast.success("已结束发送");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  return (
    <div className="flex shrink-0 items-center gap-2 border-t border-border px-3 py-2">
      <Input
        className="font-mono text-xs"
        value={draft}
        placeholder={binary ? "Base64 二进制内容" : protocol === "grpc" ? "{ }" : "消息"}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") void send();
        }}
      />
      {protocol === "websocket" ? (
        <label className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
          <input
            type="checkbox"
            className="size-3.5 accent-primary"
            checked={binary}
            onChange={(event) => setBinary(event.target.checked)}
          />
          二进制
        </label>
      ) : null}
      <Button size="sm" disabled={pending || !connectionId} onClick={() => void send()}>
        发送
      </Button>
      {clientStreaming ? (
        <Button variant="outline" size="sm" disabled={!connectionId} onClick={() => void closeStream()}>
          结束发送
        </Button>
      ) : null}
    </div>
  );
}

function statusLabel(status?: string) {
  switch (status) {
    case "connecting":
      return "连接中";
    case "open":
      return "已连接";
    case "error":
      return "出错";
    case "closed":
      return "已关闭";
    default:
      return "未连接";
  }
}
