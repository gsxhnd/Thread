import { useState } from "react";
import { Loader2Icon, SaveIcon, SendIcon, UnplugIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  graphqlSend,
  graphqlSubscribe,
  grpcInvoke,
  grpcOpenStream,
  wsConnect,
} from "@/lib/api";
import { normalizeAuth, normalizeKeyValues } from "@/lib/request-config";
import {
  docBool,
  docNumber,
  docString,
  docStringList,
  errorMessage,
  findProtoMethod,
  graphqlAsHttp,
  graphqlOperation,
  grpcAsHttp,
} from "@/lib/protocol";
import { disconnectTab } from "@/lib/protocol-session";
import { activeEnvironmentPath, useAppStore } from "@/stores/app-store";
import { useResponseStore } from "@/stores/response-store";
import { useSessionStore } from "@/stores/session-store";
import type { RequestTab } from "@/stores/tabs-store";
import { useTabsStore } from "@/stores/tabs-store";
import { patchDocument } from "@/lib/request-config";
import { VariableInput } from "./variable-input";
import { useSaveRequest } from "./use-save-request";

function environmentPath() {
  return activeEnvironmentPath(useAppStore.getState().snapshot);
}

export function SaveRequestButton({ tab }: { tab: RequestTab }) {
  const save = useSaveRequest();
  return (
    <Button
      variant="outline"
      onClick={() => void save(tab.key)}
      disabled={!tab.dirty}
      title="保存到工作区文件（Ctrl/Cmd+S）"
    >
      <SaveIcon data-icon="inline-start" />
      保存
    </Button>
  );
}

function updateDocument(tab: RequestTab, patch: Parameters<typeof patchDocument>[1]) {
  useTabsStore.getState().updateDocument(tab.key, patchDocument(tab.document, patch));
}

export function GraphqlActionBar({ tab }: { tab: RequestTab }) {
  const save = <SaveRequestButton tab={tab} />;
  const [pending, setPending] = useState(false);
  const live = useSessionStore((state) => state.byTab[tab.key]);
  const document = tab.document;
  const endpoint = docString(document, "endpoint");
  const query = docString(document, "query");
  const operation = graphqlOperation(query);
  const connected = live?.status === "open" || live?.status === "connecting";

  const handleSend = async () => {
    if (operation === "subscription" && connected) {
      await disconnectTab(tab.key, "graphql");
      return;
    }
    setPending(true);
    try {
      const payload = {
        endpoint,
        query,
        variables: docString(document, "variables") || "{}",
        operationName: docString(document, "operationName") || null,
        headers: normalizeKeyValues(document.headers),
        auth: normalizeAuth(document.auth),
        requestPath: tab.path,
        environmentPath: environmentPath(),
      };
      if (operation === "subscription") {
        const started = await graphqlSubscribe(payload);
        useSessionStore.getState().bind(tab.key, started.subscriptionId);
        return;
      }
      const response = await graphqlSend(payload);
      useResponseStore.getState().succeed(tab.key, graphqlAsHttp(response, endpoint));
      if (response.unresolvedVariables.length > 0) {
        toast.warning(`未解析变量：${response.unresolvedVariables.join(", ")}`);
      }
    } catch (error) {
      if (operation !== "subscription") useResponseStore.getState().fail(tab.key, error);
      toast.error(errorMessage(error));
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-shell-main px-3 py-2">
      <VariableInput
        className="min-w-0 flex-1"
        inputClassName="font-mono text-xs"
        value={endpoint}
        placeholder="https://api.example.com/graphql"
        withPicker
        onValueChange={(value) => updateDocument(tab, { endpoint: value })}
        onKeyDown={(event) => {
          if (event.key === "Enter") void handleSend();
        }}
      />
      <Button
        variant={connected ? "destructive" : "default"}
        disabled={pending || (!endpoint.trim() && !connected)}
        onClick={() => void handleSend()}
      >
        {pending ? (
          <Loader2Icon data-icon="inline-start" className="animate-spin" />
        ) : connected ? (
          <UnplugIcon data-icon="inline-start" />
        ) : (
          <SendIcon data-icon="inline-start" />
        )}
        {connected ? "停止" : operation === "subscription" ? "订阅" : "发送"}
      </Button>
      {save}
    </div>
  );
}

export function WebsocketActionBar({ tab }: { tab: RequestTab }) {
  const [pending, setPending] = useState(false);
  const live = useSessionStore((state) => state.byTab[tab.key]);
  const document = tab.document;
  const url = docString(document, "url");
  const connected = live?.status === "open" || live?.status === "connecting";

  const handleConnect = async () => {
    if (connected) {
      await disconnectTab(tab.key, "websocket");
      return;
    }
    setPending(true);
    try {
      const started = await wsConnect({
        url,
        headers: normalizeKeyValues(document.headers),
        protocols: docStringList(document, "protocols"),
        autoReconnect: docBool(document, "autoReconnect"),
        environmentPath: environmentPath(),
      });
      useSessionStore.getState().bind(tab.key, started.connectionId);
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-shell-main px-3 py-2">
      <VariableInput
        className="min-w-0 flex-1"
        inputClassName="font-mono text-xs"
        value={url}
        placeholder="wss://example.com/socket"
        withPicker
        onValueChange={(value) => updateDocument(tab, { url: value })}
        onKeyDown={(event) => {
          if (event.key === "Enter") void handleConnect();
        }}
      />
      <Button
        variant={connected ? "destructive" : "default"}
        disabled={pending || (!url.trim() && !connected)}
        onClick={() => void handleConnect()}
      >
        {pending ? (
          <Loader2Icon data-icon="inline-start" className="animate-spin" />
        ) : connected ? (
          <UnplugIcon data-icon="inline-start" />
        ) : (
          <SendIcon data-icon="inline-start" />
        )}
        {connected ? "断开" : "连接"}
      </Button>
      <SaveRequestButton tab={tab} />
    </div>
  );
}

export function GrpcActionBar({ tab }: { tab: RequestTab }) {
  const [pending, setPending] = useState(false);
  const live = useSessionStore((state) => state.byTab[tab.key]);
  const document = tab.document;
  const host = docString(document, "host");
  const service = docString(document, "service");
  const methodName = docString(document, "method");
  const method = findProtoMethod(live?.services ?? [], service, methodName);
  const streaming = !!method && (method.clientStreaming || method.serverStreaming);
  const connected = live?.status === "open" || live?.status === "connecting";
  const tls = docBool(document, "tlsEnabled");

  const handleCall = async () => {
    if (streaming && connected) {
      await disconnectTab(tab.key, "grpc");
      return;
    }
    const poolId = live?.poolId;
    if (!poolId) {
      toast.error("请先加载 .proto 文件或执行服务反射");
      return;
    }
    setPending(true);
    const payloadText = docString(document, "payload") || "{}";
    const call = {
      poolId,
      host,
      service,
      method: methodName,
      payload: payloadText,
      metadata: normalizeKeyValues(document.metadata),
      tlsEnabled: tls,
      timeoutMs: docNumber(document, "timeoutMs", 30_000),
      environmentPath: environmentPath(),
      requestPath: tab.path,
    };
    try {
      if (streaming) {
        const started = await grpcOpenStream(call);
        useSessionStore.getState().bind(tab.key, started.streamId);
        if (payloadText.trim()) {
          useSessionStore.getState().pushByTab(tab.key, {
            direction: "out",
            text: payloadText,
            at: Date.now(),
          });
        }
        return;
      }
      const result = await grpcInvoke(call);
      const body =
        typeof result.response === "string"
          ? result.response
          : JSON.stringify(result.response, null, 2);
      useResponseStore.getState().succeed(tab.key, grpcAsHttp(body, host, result.durationMs));
    } catch (error) {
      if (!streaming) useResponseStore.getState().fail(tab.key, error);
      toast.error(errorMessage(error));
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-shell-main px-3 py-2">
      <VariableInput
        className="min-w-0 flex-1"
        inputClassName="font-mono text-xs"
        value={host}
        placeholder="127.0.0.1:50051"
        withPicker
        onValueChange={(value) => updateDocument(tab, { host: value })}
        onKeyDown={(event) => {
          if (event.key === "Enter") void handleCall();
        }}
      />
      <Button
        variant={tls ? "secondary" : "outline"}
        onClick={() => updateDocument(tab, { tlsEnabled: !tls })}
        title="使用 TLS 连接"
      >
        TLS
      </Button>
      <Button
        variant={connected ? "destructive" : "default"}
        disabled={pending || (!host.trim() && !connected)}
        onClick={() => void handleCall()}
      >
        {pending ? <Loader2Icon data-icon="inline-start" className="animate-spin" /> : null}
        {connected ? "停止" : streaming ? "开始" : "调用"}
      </Button>
      <SaveRequestButton tab={tab} />
    </div>
  );
}
