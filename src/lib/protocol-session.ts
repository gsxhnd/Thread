import {
  graphqlUnsubscribe,
  grpcCancelStream,
  onGraphqlSubscription,
  onGrpcStream,
  onWsMessage,
  onWsStatus,
  wsDisconnect,
} from "@/lib/api";
import { formatEventData } from "@/lib/protocol";
import { useSessionStore, type LiveStatus } from "@/stores/session-store";
import { useTabsStore } from "@/stores/tabs-store";

function wsStatus(status: string): LiveStatus {
  if (status === "open") return "open";
  if (status === "connecting" || status === "reconnecting" || status === "closing") return "connecting";
  if (status === "error") return "error";
  return "closed";
}

export async function disconnectTab(tabKey: string, protocol: string, drop = false) {
  const session = useSessionStore.getState().byTab[tabKey];
  const connectionId = session?.connectionId;
  if (drop) useSessionStore.getState().forget(tabKey);
  else useSessionStore.getState().release(tabKey);
  if (!connectionId) return;
  try {
    if (protocol === "websocket") await wsDisconnect(connectionId);
    else if (protocol === "graphql") await graphqlUnsubscribe(connectionId);
    else if (protocol === "grpc") await grpcCancelStream(connectionId);
  } catch {
    // 连接可能已经结束。
  }
}

export async function disconnectAll() {
  const tabs = useTabsStore.getState().tabs;
  const protocolByKey = new Map(tabs.map((tab) => [tab.key, tab.protocol]));
  const sessions = useSessionStore.getState().connectionIds();
  await Promise.all(
    sessions.map((item) => disconnectTab(item.tabKey, protocolByKey.get(item.tabKey) ?? "")),
  );
}

let started = false;

/** 工作区存续期间接收长连接事件，并在标签页关闭时释放连接。 */
export function startProtocolSessions() {
  if (started) return () => {};
  started = true;

  const stopTabs = useTabsStore.subscribe((state, previous) => {
    const nextKeys = new Set(state.tabs.map((tab) => tab.key));
    for (const tab of previous.tabs) {
      if (!nextKeys.has(tab.key)) void disconnectTab(tab.key, tab.protocol, true);
    }
  });

  const stopWsMessage = onWsMessage((event) => {
    useSessionStore.getState().pushByConnection(event.connectionId, {
      direction: event.direction === "out" ? "out" : "in",
      text: event.isBinary ? `[binary] ${event.payload}` : event.payload,
      at: event.timestamp || Date.now(),
    });
  });
  const stopWsStatus = onWsStatus((event) => {
    useSessionStore.getState().setStatus(
      event.connectionId,
      wsStatus(event.status),
      event.reason ?? event.status,
    );
  });
  const stopGraphql = onGraphqlSubscription((event) => {
    const store = useSessionStore.getState();
    if (event.event === "ack") {
      store.setStatus(event.subscriptionId, "open", "已订阅");
      return;
    }
    if (event.event === "complete") {
      store.pushByConnection(event.subscriptionId, {
        direction: "status",
        text: "订阅已结束",
        at: Date.now(),
      });
      store.setStatus(event.subscriptionId, "closed");
      return;
    }
    if (event.event === "error") {
      store.pushByConnection(event.subscriptionId, {
        direction: "status",
        text: formatEventData(event.data) || "订阅出错",
        at: Date.now(),
      });
      store.setStatus(event.subscriptionId, "error");
      return;
    }
    store.pushByConnection(event.subscriptionId, {
      direction: "in",
      text: formatEventData(event.data),
      at: Date.now(),
    });
  });
  const stopGrpc = onGrpcStream((event) => {
    const store = useSessionStore.getState();
    if (event.error) {
      store.pushByConnection(event.streamId, {
        direction: "status",
        text: event.error,
        at: Date.now(),
      });
      store.setStatus(event.streamId, "error");
    } else if (event.data !== undefined && event.data !== null) {
      store.pushByConnection(event.streamId, {
        direction: event.direction === "out" ? "out" : "in",
        text: formatEventData(event.data),
        at: Date.now(),
      });
      if (store.byTab[store.tabByConnection[event.streamId] ?? ""]?.status !== "open") {
        store.setStatus(event.streamId, "open");
      }
    }
    if (event.done) {
      store.pushByConnection(event.streamId, {
        direction: "status",
        text: "流已结束",
        at: Date.now(),
      });
      store.setStatus(event.streamId, "closed");
    }
  });

  return () => {
    started = false;
    stopTabs();
    stopWsMessage();
    stopWsStatus();
    stopGraphql();
    stopGrpc();
    void disconnectAll();
  };
}
