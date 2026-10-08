/** 工作区生命周期 IPC 封装（docs/04.ipc-api.md）。 */
import { invokeCmd } from "./client";
import type {
  AppPaths,
  SearchHit,
  SessionData,
  WorkspaceSnapshot,
} from "./types";

export function workspaceCreate(input: {
  path: string;
  name?: string;
}): Promise<WorkspaceSnapshot> {
  return invokeCmd("workspace_create", input);
}

export function workspaceInit(input: {
  path: string;
  name?: string;
}): Promise<WorkspaceSnapshot> {
  return invokeCmd("workspace_init", input);
}

export function workspaceOpen(input: {
  path: string;
}): Promise<WorkspaceSnapshot> {
  return invokeCmd("workspace_open", input);
}

export function workspaceCurrent(): Promise<WorkspaceSnapshot | null> {
  return invokeCmd("workspace_current");
}

export function workspaceClose(): Promise<void> {
  return invokeCmd("workspace_close");
}

export function workspaceRefresh(): Promise<WorkspaceSnapshot> {
  return invokeCmd("workspace_refresh");
}

export function workspaceUpdateSettings(input: {
  name?: string;
  settings?: WorkspaceSnapshot["config"]["settings"];
  proxy?: WorkspaceSnapshot["config"]["proxy"];
  activeEnvironment?: string | null;
}): Promise<WorkspaceSnapshot> {
  return invokeCmd("workspace_update_settings", input);
}

export function workspaceSearch(input: {
  query: string;
  limit?: number;
}): Promise<SearchHit[]> {
  return invokeCmd("workspace_search", input);
}

export function sessionGet(): Promise<SessionData> {
  return invokeCmd("session_get");
}

export function sessionForgetWorkspace(path: string): Promise<SessionData> {
  return invokeCmd("session_forget_workspace", { path });
}

export function appPaths(): Promise<AppPaths> {
  return invokeCmd("app_paths");
}
