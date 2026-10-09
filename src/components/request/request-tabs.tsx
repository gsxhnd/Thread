import { SendIcon, XIcon, Loader2Icon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { SelectField } from "@/components/ui/select-field";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { HTTP_METHODS, methodColor } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useResponseStore } from "@/stores/response-store";
import { useTabsStore, type RequestTab } from "@/stores/tabs-store";
import { patchDocument } from "@/lib/request-config";
import { VariableInput } from "./variable-input";
import { useSendRequest } from "./use-send-request";
import { GraphqlActionBar, GrpcActionBar, SaveRequestButton, WebsocketActionBar } from "./protocol-bar";

function protocolBadge(protocol: string) {
  if (protocol === "graphql") return "GQL";
  if (protocol === "websocket") return "WS";
  if (protocol === "grpc") return "gRPC";
  return protocol;
}
export function RequestTabs() {
  const tabs = useTabsStore((state) => state.tabs);
  const activeKey = useTabsStore((state) => state.activeKey);
  const setActive = useTabsStore((state) => state.setActive);
  const closeTab = useTabsStore((state) => state.closeTab);
  const closeOthers = useTabsStore((state) => state.closeOthers);
  const closeAll = useTabsStore((state) => state.closeAll);

  if (tabs.length === 0) return null;

  return (
    <div className="flex h-10 shrink-0 items-end gap-0.5 overflow-x-auto border-b border-border bg-shell-secondary px-2">
      {tabs.map((tab) => {
        const active = tab.key === activeKey;
        return (
          <ContextMenu key={tab.key}>
          <ContextMenuTrigger
            className={cn(
              "group mb-[-1px] flex shrink-0 items-center gap-1 rounded-t-lg border border-b-0 px-2 text-sm",
              active
                ? "border-border bg-shell-main text-foreground"
                : "border-transparent text-muted-foreground hover:bg-muted/70 hover:text-foreground",
            )}
          >
            <button
              type="button"
              className="flex max-w-56 items-center gap-1.5 py-1.5"
              title={tab.path ?? "未保存的草稿"}
              onClick={() => setActive(tab.key)}
            >
              <span
                className={cn(
                  "text-[10px] font-semibold tracking-wide uppercase",
                  methodColor(tab.document.method),
                )}
              >
                {tab.protocol === "http" ? (tab.document.method ?? "GET") : protocolBadge(tab.protocol)}
              </span>
              <span className="truncate">{tab.name}</span>
              {tab.dirty ? (
                <span className="size-1.5 rounded-full bg-notice" title="未保存" />
              ) : null}
            </button>
            <button
              type="button"
              className={cn(
                "rounded-md p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground",
                active ? "opacity-100" : "opacity-0 group-hover:opacity-100",
              )}
              title="关闭标签页"
              onClick={() => closeTab(tab.key)}
            >
              <XIcon className="size-3" />
            </button>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem onClick={() => closeTab(tab.key)}>关闭</ContextMenuItem>
            <ContextMenuItem onClick={() => closeOthers(tab.key)}>关闭其他</ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem onClick={() => closeAll()}>关闭全部</ContextMenuItem>
          </ContextMenuContent>
          </ContextMenu>
        );
      })}
    </div>
  );
}

/** 激活标签页的操作栏。HTTP 保留方法/URL；其他协议各自连接或发送。 */
export function RequestActionBar({ tab }: { tab: RequestTab }) {
  if (tab.protocol === "graphql") return <GraphqlActionBar tab={tab} />;
  if (tab.protocol === "websocket") return <WebsocketActionBar tab={tab} />;
  if (tab.protocol === "grpc") return <GrpcActionBar tab={tab} />;
  return <HttpActionBar tab={tab} />;
}

function HttpActionBar({ tab }: { tab: RequestTab }) {
  const { send, cancel } = useSendRequest();
  const responseState = useResponseStore((state) => state.byTab[tab.key]);
  const sending = responseState?.status === "loading";
  const document = tab.document;

  const update = (patch: Parameters<typeof patchDocument>[1]) => {
    useTabsStore.getState().updateDocument(tab.key, patchDocument(document, patch));
  };

  const handleSend = async () => {
    if (sending && responseState?.status === "loading") {
      await cancel(responseState.taskId);
      return;
    }
    const response = await send(tab.key);
    if (!response) {
      const failed = useResponseStore.getState().byTab[tab.key];
      if (failed && failed.status === "error") {
        toast.error(`${failed.code}: ${failed.message}`);
      }
    }
  };

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-shell-main px-3 py-2">
      <div className="flex min-w-0 flex-1 items-center rounded-lg border border-input bg-background focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50 [&_[data-slot=input]]:rounded-none [&_[data-slot=input]]:border-0 [&_[data-slot=input]]:bg-transparent [&_[data-slot=input]]:shadow-none [&_[data-slot=input]]:focus-visible:ring-0">
        <SelectField
          aria-label="请求方法"
          className={cn(
            "w-[6.75rem] shrink-0 rounded-none border-0 bg-transparent font-semibold shadow-none focus-visible:ring-0 dark:bg-transparent",
            methodColor(document.method),
          )}
          value={document.method ?? "GET"}
          onValueChange={(method) => update({ method })}
          options={HTTP_METHODS.map((method) => ({ value: method, label: method }))}
        />
        <span className="h-4 w-px shrink-0 bg-border" />
        <VariableInput
          className="min-w-0 flex-1"
          inputClassName="font-mono text-xs"
          value={document.url ?? ""}
          placeholder="https://api.example.com/users 或 {{baseUrl}}/users"
          withPicker
          onValueChange={(url) => update({ url })}
          onKeyDown={(event) => {
            if (event.key === "Enter") void handleSend();
          }}
        />
      </div>

      <Button
        variant={sending ? "destructive" : "default"}
        onClick={() => void handleSend()}
        disabled={!document.url?.trim() && !sending}
        title={sending ? "取消请求" : "发送请求（Enter）"}
      >
        {sending ? (
          <Loader2Icon data-icon="inline-start" className="animate-spin" />
        ) : (
          <SendIcon data-icon="inline-start" />
        )}
        {sending ? "取消" : "发送"}
      </Button>

      <SaveRequestButton tab={tab} />
    </div>
  );
}
