import { invokeCmd } from "@/lib/api/client";
import type { KeyValue } from "@/lib/api/types";

export interface WsConnectPayload {
  url: string;
  headers?: KeyValue[];
  protocols?: string[];
  autoReconnect?: boolean;
  environmentPath?: string | null;
  collectionPath?: string | null;
}

export function wsConnect(payload: WsConnectPayload) {
  return invokeCmd<{ connectionId: string }>("ws_connect", payload);
}

export function wsSend(payload: { connectionId: string; message: string; isBinary?: boolean }) {
  return invokeCmd<void>("ws_send", payload);
}

export function wsDisconnect(connectionId: string) {
  return invokeCmd<void>("ws_disconnect", { connectionId });
}
