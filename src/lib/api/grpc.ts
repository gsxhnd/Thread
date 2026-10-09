import { invokeCmd } from "@/lib/api/client";
import type { GrpcResult, KeyValue, ProtoDefinition } from "@/lib/api/types";

export interface GrpcCallPayload {
  poolId: string;
  host: string;
  service: string;
  method: string;
  payload?: string;
  metadata?: KeyValue[];
  tlsEnabled?: boolean;
  timeoutMs?: number | null;
  environmentPath?: string | null;
  collectionPath?: string | null;
  requestPath?: string | null;
}

export function grpcLoadProto(filePath: string) {
  return invokeCmd<ProtoDefinition>("grpc_load_proto", { filePath });
}

export function grpcReflect(payload: {
  host: string;
  tlsEnabled?: boolean;
  metadata?: KeyValue[];
  environmentPath?: string | null;
}) {
  return invokeCmd<ProtoDefinition>("grpc_reflect", payload);
}

export function grpcInvoke(payload: GrpcCallPayload) {
  return invokeCmd<GrpcResult>("grpc_invoke", payload);
}

export function grpcOpenStream(payload: GrpcCallPayload) {
  return invokeCmd<{ streamId: string }>("grpc_open_stream", payload);
}

export function grpcStreamSend(payload: { streamId: string; payload: string }) {
  return invokeCmd<void>("grpc_stream_send", payload);
}

export function grpcStreamClose(streamId: string) {
  return invokeCmd<void>("grpc_stream_close", { streamId });
}

export function grpcCancelStream(streamId: string) {
  return invokeCmd<void>("grpc_cancel_stream", { streamId });
}
