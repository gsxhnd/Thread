/** HTTP 发送 / 取消 IPC 封装。 */
import { invokeCmd } from "./client";
import type { HttpRequestConfig, HttpResponse } from "./types";

export function httpSend(input: {
  taskId: string;
  requestPath?: string | null;
  config: HttpRequestConfig;
  environmentPath?: string | null;
  saveHistory?: boolean;
}): Promise<HttpResponse> {
  return invokeCmd("http_send", input);
}

export function httpCancel(taskId: string): Promise<{ cancelled: boolean }> {
  return invokeCmd("http_cancel", { taskId });
}

export function httpActiveTasks(): Promise<string[]> {
  return invokeCmd("http_active_tasks");
}

/** 前端生成 taskId，使请求在飞行中即可被取消（Rust 侧按 id 注册取消信号）。 */
export function newTaskId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `task-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}
