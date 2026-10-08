import type {
  AuthSpec,
  HttpBody,
  HttpRequestConfig,
  KeyValue,
  RequestDocument,
  RequestSettings,
} from "@/lib/api";

/** 把工作区文档归一化为发送用的 HTTP 配置（缺失字段回落到默认值）。 */
export function toHttpConfig(document: RequestDocument): HttpRequestConfig {
  return {
    method: (document.method ?? "GET").toUpperCase(),
    url: document.url ?? "",
    queryParams: normalizeKeyValues(document.queryParams),
    headers: normalizeKeyValues(document.headers),
    body: normalizeBody(document.body),
    auth: normalizeAuth(document.auth),
    settings: normalizeSettings(document.settings),
  };
}

export function normalizeKeyValues(value: unknown): KeyValue[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter((item): item is Record<string, unknown> => !!item && typeof item === "object")
    .map((item) => ({
      key: typeof item.key === "string" ? item.key : "",
      value: typeof item.value === "string" ? item.value : "",
      enabled: item.enabled !== false,
      description:
        typeof item.description === "string" ? item.description : undefined,
    }));
}

export function normalizeBody(value: unknown): HttpBody {
  if (!value || typeof value !== "object") {
    return { type: "none", content: "" };
  }
  const body = value as Record<string, unknown>;
  const rawFields = Array.isArray(body.fields) ? body.fields : [];
  return {
    type: typeof body.type === "string" ? body.type : "none",
    content: typeof body.content === "string" ? body.content : "",
    contentType:
      typeof body.contentType === "string" ? body.contentType : undefined,
    filePath: typeof body.filePath === "string" ? body.filePath : undefined,
    fields: rawFields
      .filter((item): item is Record<string, unknown> => !!item && typeof item === "object")
      .map((item) => ({
        key: typeof item.key === "string" ? item.key : "",
        value: typeof item.value === "string" ? item.value : "",
        kind: item.kind === "file" ? "file" : "text",
        enabled: item.enabled !== false,
        filePath: typeof item.filePath === "string" ? item.filePath : undefined,
        contentType:
          typeof item.contentType === "string" ? item.contentType : undefined,
      })),
  };
}

export function normalizeAuth(value: unknown): AuthSpec {
  if (!value || typeof value !== "object") return { type: "inherit" };
  const auth = value as Record<string, unknown>;
  return {
    type: typeof auth.type === "string" ? auth.type : "inherit",
    token: typeof auth.token === "string" ? auth.token : undefined,
    username: typeof auth.username === "string" ? auth.username : undefined,
    password: typeof auth.password === "string" ? auth.password : undefined,
    key: typeof auth.key === "string" ? auth.key : undefined,
    value: typeof auth.value === "string" ? auth.value : undefined,
    in: auth.in === "query" ? "query" : "header",
  };
}

export function normalizeSettings(value: unknown): RequestSettings {
  if (!value || typeof value !== "object") return {};
  const settings = value as Record<string, unknown>;
  return {
    timeoutMs:
      typeof settings.timeoutMs === "number" ? settings.timeoutMs : undefined,
    followRedirects:
      typeof settings.followRedirects === "boolean"
        ? settings.followRedirects
        : undefined,
    verifySsl:
      typeof settings.verifySsl === "boolean" ? settings.verifySsl : undefined,
  };
}

export function createDraftDocument(name: string): RequestDocument {
  return {
    version: 1,
    protocol: "http",
    name,
    method: "GET",
    url: "",
    queryParams: [],
    headers: [],
    body: { type: "none", content: "" },
    auth: { type: "inherit" },
    settings: {},
  };
}

/** 不可变更新：返回新的文档对象。 */
export function patchDocument(
  document: RequestDocument,
  patch: Partial<RequestDocument>,
): RequestDocument {
  return { ...document, ...patch };
}

export function bodyHasContent(body: HttpBody): boolean {
  switch (body.type) {
    case "json":
    case "raw":
      return body.content.trim().length > 0;
    case "formUrlencoded":
    case "multipart":
      return (body.fields ?? []).some((field) => field.enabled && field.key.trim());
    case "binary":
      return !!body.filePath?.trim();
    default:
      return false;
  }
}
