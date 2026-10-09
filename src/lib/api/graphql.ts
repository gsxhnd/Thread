import { invokeCmd } from "@/lib/api/client";
import type { GraphqlResponse, GraphqlSchema, KeyValue } from "@/lib/api/types";

export interface GraphqlSendPayload {
  endpoint: string;
  query: string;
  variables?: string;
  headers?: KeyValue[];
  auth?: unknown;
  operationName?: string | null;
  environmentPath?: string | null;
  collectionPath?: string | null;
  requestPath?: string | null;
}

export function graphqlSend(payload: GraphqlSendPayload) {
  return invokeCmd<GraphqlResponse>("graphql_send", payload);
}

export function graphqlIntrospect(payload: {
  endpoint: string;
  headers?: KeyValue[];
  auth?: unknown;
  environmentPath?: string | null;
  collectionPath?: string | null;
}) {
  return invokeCmd<GraphqlSchema>("graphql_introspect", payload);
}

export function graphqlSubscribe(payload: GraphqlSendPayload) {
  return invokeCmd<{ subscriptionId: string }>("graphql_subscribe", payload);
}

export function graphqlUnsubscribe(subscriptionId: string) {
  return invokeCmd<void>("graphql_unsubscribe", { subscriptionId });
}
