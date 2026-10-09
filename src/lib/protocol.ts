import type {
  GraphqlResponse,
  GraphqlSchema,
  GraphqlTypeRef,
  HttpResponse,
  ProtoServiceInfo,
  RequestDocument,
} from "@/lib/api";
import { ApiError } from "@/lib/api";

export function docString(document: RequestDocument, key: string): string {
  const value = document[key];
  return typeof value === "string" ? value : "";
}

export function docBool(document: RequestDocument, key: string): boolean {
  return document[key] === true;
}

export function docNumber(document: RequestDocument, key: string, fallback: number): number {
  const value = document[key];
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export function docStringList(document: RequestDocument, key: string): string[] {
  const value = document[key];
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string");
}

/** 与 Rust `graphql_operation` 一致：忽略 `#` 注释后看第一个单词。 */
export function graphqlOperation(query: string): "query" | "mutation" | "subscription" {
  const stripped = query.replace(/#[^\n]*/g, " ").trim().split(/\s+/)[0]?.toLowerCase() ?? "";
  if (stripped === "subscription" || stripped === "mutation") return stripped;
  return "query";
}

export function graphqlAsHttp(response: GraphqlResponse, endpoint: string): HttpResponse {
  return {
    taskId: "",
    url: endpoint,
    statusCode: response.statusCode,
    statusText: response.statusText,
    headers: response.headers,
    body: response.body,
    bodySize: new TextEncoder().encode(response.body).length,
    contentType: "application/json",
    timing: {
      dnsMs: 0,
      tcpMs: 0,
      tlsMs: 0,
      ttfbMs: 0,
      downloadMs: 0,
      totalMs: response.durationMs,
    },
    redirectCount: 0,
    unresolvedVariables: response.unresolvedVariables,
  };
}

export function grpcAsHttp(body: string, host: string, durationMs: number): HttpResponse {
  return {
    taskId: "",
    url: host,
    statusCode: 200,
    statusText: "OK",
    headers: {},
    body,
    bodySize: new TextEncoder().encode(body).length,
    contentType: "application/json",
    timing: {
      dnsMs: 0,
      tcpMs: 0,
      tlsMs: 0,
      ttfbMs: 0,
      downloadMs: 0,
      totalMs: durationMs,
    },
    redirectCount: 0,
    unresolvedVariables: [],
  };
}

export function formatEventData(data: unknown): string {
  if (data == null) return "";
  if (typeof data === "string") return data;
  try {
    return JSON.stringify(data, null, 2);
  } catch {
    return String(data);
  }
}

function unwrapType(type?: GraphqlTypeRef | null): GraphqlTypeRef | undefined {
  let current = type ?? undefined;
  while (current && (current.kind === "NON_NULL" || current.kind === "LIST")) {
    current = current.ofType ?? undefined;
  }
  return current;
}

export function schemaRootFields(
  schema: GraphqlSchema | null | undefined,
  operation: "query" | "mutation" | "subscription",
) {
  if (!schema) return [];
  const name =
    operation === "mutation"
      ? schema.mutationType?.name
      : operation === "subscription"
        ? schema.subscriptionType?.name
        : schema.queryType?.name;
  if (!name) return [];
  return schema.types?.find((item) => item.name === name)?.fields ?? [];
}

export function findProtoMethod(
  services: ProtoServiceInfo[],
  service: string,
  method: string,
) {
  return services
    .find((item) => item.name === service)
    ?.methods.find((item) => item.name === method);
}

export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return `${error.code}: ${error.message}`;
  if (error instanceof Error) return error.message;
  return String(error);
}

export function insertGraphqlField(
  query: string,
  operation: string,
  fieldName: string,
  fieldType?: GraphqlTypeRef | null,
): string {
  const leaf = unwrapType(fieldType);
  const selection =
    leaf?.kind === "OBJECT" || leaf?.kind === "INTERFACE" || leaf?.kind === "UNION"
      ? `${fieldName} {\n    __typename\n  }`
      : fieldName;
  if (!query.trim()) {
    return `${operation} {\n  ${selection}\n}\n`;
  }
  return `${query.replace(/\s+$/, "")}\n${selection}\n`;
}
