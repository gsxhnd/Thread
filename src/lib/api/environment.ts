/** 环境与变量 IPC 封装。 */
import { invokeCmd } from "./client";
import type {
  EnvironmentDetail,
  EnvironmentSummary,
  TrashResult,
  VariablePreview,
  WorkspaceSnapshot,
} from "./types";

export function environmentList(): Promise<EnvironmentSummary[]> {
  return invokeCmd("environment_list");
}

export function environmentGet(path: string): Promise<EnvironmentDetail> {
  return invokeCmd("environment_get", { path });
}

export function environmentCreate(name: string): Promise<EnvironmentDetail> {
  return invokeCmd("environment_create", { name });
}

export function environmentUpdate(input: {
  path: string;
  name?: string;
}): Promise<EnvironmentDetail> {
  return invokeCmd("environment_update", input);
}

export function environmentDelete(path: string): Promise<TrashResult> {
  return invokeCmd("environment_delete", { path });
}

export function environmentActivate(path: string): Promise<WorkspaceSnapshot> {
  return invokeCmd("environment_activate", { path });
}

export function envVariableSet(input: {
  environmentPath: string;
  key: string;
  value?: string;
  enabled?: boolean;
  secret?: boolean;
  description?: string;
}): Promise<EnvironmentDetail> {
  return invokeCmd("env_variable_set", input);
}

export function envVariableDelete(input: {
  environmentPath: string;
  key: string;
}): Promise<EnvironmentDetail> {
  return invokeCmd("env_variable_delete", input);
}

/** 显式读取机密变量值（用户主动点击「显示」时才会调用）。 */
export function envVariableReveal(input: {
  environmentPath: string;
  key: string;
}): Promise<string | null> {
  return invokeCmd("env_variable_reveal", input);
}

export function variablesPreview(input: {
  environmentPath?: string | null;
  text: string;
}): Promise<VariablePreview> {
  return invokeCmd("variables_preview", input);
}
