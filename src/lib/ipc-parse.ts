/**
 * Runtime validation helpers for IPC invoke responses.
 *
 * IPC 返回值运行时校验辅助函数，用于在 `invoke<unknown>` 之后安全解析结构。
 */

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string");
}
