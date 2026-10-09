/**
 * IPC 类型定义 —— 与 Rust 侧 serde 结构一一对应（camelCase）。
 *
 * 约定：所有 command 走 `invokeCmd`（DTO 放在 `payload` 里），
 * 组件禁止直接调用 `invoke`（见 docs/04.ipc-api.md §6）。
 */

export interface AppError {
  code: string;
  message: string;
}

export type Protocol = "http" | "grpc" | "websocket" | "graphql" | "mqtt";

export interface KeyValue {
  key: string;
  value: string;
  enabled: boolean;
  description?: string | null;
}

export interface RequestSettings {
  timeoutMs?: number | null;
  followRedirects?: boolean | null;
  verifySsl?: boolean | null;
}

export type AuthType = "inherit" | "none" | "bearer" | "basic" | "apiKey" | "oauth2";

export interface AuthSpec {
  type: AuthType | string;
  token?: string | null;
  username?: string | null;
  password?: string | null;
  key?: string | null;
  value?: string | null;
  in?: "header" | "query" | null;
}

export interface HttpBodyField {
  key: string;
  value: string;
  kind: "text" | "file";
  enabled: boolean;
  filePath?: string | null;
  contentType?: string | null;
}

export type HttpBodyType =
  | "none"
  | "json"
  | "raw"
  | "formUrlencoded"
  | "multipart"
  | "binary";

export interface HttpBody {
  type: HttpBodyType | string;
  content: string;
  contentType?: string | null;
  fields?: HttpBodyField[];
  filePath?: string | null;
}

export interface HttpRequestConfig {
  method: string;
  url: string;
  queryParams: KeyValue[];
  headers: KeyValue[];
  body: HttpBody;
  auth: AuthSpec;
  settings: RequestSettings;
}

/** 工作区文件里保存的请求文档（原样读写，不丢字段）。 */
export interface RequestDocument extends Partial<HttpRequestConfig> {
  version?: number;
  id?: string;
  protocol: Protocol | string;
  name: string;
  method?: string;
  url?: string;
  [key: string]: unknown;
}

export interface RequestDetail {
  id: string;
  name: string;
  protocol: Protocol | string;
  path: string;
  collectionPath: string;
  collectionName: string;
  inheritedHeaders: KeyValue[];
  inheritedAuth: AuthSpec;
  raw: RequestDocument;
  http?: HttpRequestConfig | null;
  error?: string | null;
}

export interface RequestNode {
  id: string;
  name: string;
  protocol: Protocol | string;
  path: string;
  method?: string | null;
  url?: string | null;
  error?: string | null;
}

export interface CollectionNode {
  id: string;
  name: string;
  path: string;
  description?: string | null;
  hasMeta: boolean;
  requests: RequestNode[];
  children: CollectionNode[];
}

export interface EnvironmentSummary {
  id: string;
  name: string;
  path: string;
  variableCount: number;
  secretCount: number;
  /** 变量名（非机密）：用于 `{{variable}}` 自动补全。 */
  variableNames: string[];
  /** 值为本地存储（界面掩码显示）的变量名。 */
  secretVariableNames: string[];
  isGlobal: boolean;
  isActive: boolean;
}

export interface WorkspaceIndex {
  id: string;
  name: string;
  version: number;
  root: string;
  activeEnvironment?: string | null;
  collections: CollectionNode[];
  environments: EnvironmentSummary[];
  collectionCount: number;
  requestCount: number;
  warnings: string[];
  scannedAt: number;
  digest: string;
}

export interface ProxySettings {
  port: number;
  autoStart: boolean;
}

export interface WorkspaceConfig {
  version: number;
  id?: string | null;
  name: string;
  activeEnvironment?: string | null;
  proxy: ProxySettings;
  settings: RequestSettings;
}

export interface WorkspaceSnapshot {
  index: WorkspaceIndex;
  config: WorkspaceConfig;
  warnings: string[];
}

export interface CollectionDetail {
  id: string;
  name: string;
  path: string;
  description?: string | null;
  hasMeta: boolean;
  auth: AuthSpec;
  headers: KeyValue[];
  preScript: string;
  postScript: string;
  parentPath?: string | null;
  requestCount: number;
  childCount: number;
}

export interface EnvVariableView {
  key: string;
  value: string;
  enabled: boolean;
  secret: boolean;
  description?: string | null;
  hasStoredValue: boolean;
}

export interface EnvironmentDetail {
  id: string;
  name: string;
  path: string;
  isGlobal: boolean;
  isActive: boolean;
  variables: EnvVariableView[];
}

export interface SearchHit {
  id: string;
  name: string;
  protocol: Protocol | string;
  path: string;
  collectionPath: string;
  collectionName: string;
  method?: string | null;
  url?: string | null;
}

export interface Timing {
  dnsMs: number;
  tcpMs: number;
  tlsMs: number;
  ttfbMs: number;
  downloadMs: number;
  totalMs: number;
}

export interface HttpResponse {
  taskId: string;
  url: string;
  statusCode: number;
  statusText: string;
  headers: Record<string, string[]>;
  body: string | null;
  bodyFilePath?: string | null;
  bodySize: number;
  contentType?: string | null;
  timing: Timing;
  redirectCount: number;
  unresolvedVariables: string[];
}

export interface GraphqlResponse {
  statusCode: number;
  statusText: string;
  headers: Record<string, string[]>;
  body: string;
  durationMs: number;
  unresolvedVariables: string[];
}

export interface GraphqlTypeRef {
  kind?: string | null;
  name?: string | null;
  ofType?: GraphqlTypeRef | null;
}

export interface GraphqlField {
  name: string;
  args?: { name: string; type?: GraphqlTypeRef | null }[];
  type?: GraphqlTypeRef | null;
}

export interface GraphqlSchema {
  queryType?: { name?: string | null } | null;
  mutationType?: { name?: string | null } | null;
  subscriptionType?: { name?: string | null } | null;
  types?: {
    kind?: string;
    name?: string;
    fields?: GraphqlField[] | null;
  }[];
}

export interface ProtoMethodInfo {
  name: string;
  fullName: string;
  clientStreaming: boolean;
  serverStreaming: boolean;
  requestType: string;
  responseType: string;
  requestTemplate: string;
}

export interface ProtoServiceInfo {
  name: string;
  methods: ProtoMethodInfo[];
}

export interface ProtoDefinition {
  poolId: string;
  services: ProtoServiceInfo[];
}

export interface GrpcResult {
  service: string;
  method: string;
  response: unknown;
  durationMs: number;
}

export interface WsMessageEvent {
  connectionId: string;
  direction: "in" | "out" | string;
  isBinary: boolean;
  payload: string;
  timestamp: number;
}

export interface WsStatusEvent {
  connectionId: string;
  status: string;
  reason?: string | null;
}

export interface SubscriptionEvent {
  subscriptionId: string;
  event: string;
  data?: unknown;
}

export interface GrpcStreamEvent {
  streamId: string;
  direction: string;
  data?: unknown;
  done: boolean;
  error?: string | null;
}

export interface HistorySummary {
  id: string;
  protocol: string;
  name?: string | null;
  method?: string | null;
  url?: string | null;
  statusCode?: number | null;
  durationMs?: number | null;
  createdAt: number;
  requestFilePath?: string | null;
  hasBodyFile: boolean;
}

export interface HistoryDetail extends HistorySummary {
  workspacePath: string;
  requestSnapshot: {
    protocol?: string;
    name?: string;
    document?: RequestDocument;
    resolved?: {
      method?: string;
      url?: string;
      headers?: KeyValue[];
      bodyKind?: string;
      bodyPreview?: string | null;
    };
  };
  responseSnapshot?: Record<string, unknown> | null;
  responseBody?: string | null;
  responseBodyFilePath?: string | null;
}

export interface HistoryPage {
  items: HistorySummary[];
  total: number;
}

export interface AppSettings {
  defaultTimeoutMs: number;
  followRedirects: boolean;
  verifySsl: boolean;
  maxInlineBodyBytes: number;
  historyLimit: number;
  historyBodyStorage: string;
}

export interface RecentWorkspace {
  path: string;
  name: string;
  lastOpenedAt: number;
}

export interface SessionData {
  lastWorkspacePath?: string | null;
  recentWorkspaces: RecentWorkspace[];
}

export interface AppPaths {
  configDir: string;
  configFile: string;
  dataDir: string;
  dbFile: string;
}

export interface TrashResult {
  trashedPath: string;
}

export interface VariablePreview {
  resolved: string;
  missing: string[];
}

export interface WorkspaceChangedEvent {
  reason: string;
  requestCount: number;
  collectionCount: number;
  digest: string;
}

export interface WorkspaceOpenedEvent {
  path: string;
  name: string;
}
