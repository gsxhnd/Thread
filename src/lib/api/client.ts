import { invoke } from "@tauri-apps/api/core";
import type { AppError } from "./types";

export class ApiError extends Error {
  code: string;

  constructor(error: AppError) {
    super(error.message);
    this.name = "ApiError";
    this.code = error.code;
  }
}

function parseError(err: unknown): AppError {
  if (typeof err === "string") {
    try {
      const parsed = JSON.parse(err) as AppError;
      if (parsed?.code && parsed?.message) return parsed;
    } catch {
      /* fallthrough */
    }
    return { code: "UNKNOWN", message: err };
  }
  if (err && typeof err === "object") {
    const obj = err as Record<string, unknown>;
    if (typeof obj.code === "string" && typeof obj.message === "string") {
      return { code: obj.code, message: obj.message };
    }
  }
  return { code: "UNKNOWN", message: String(err) };
}

export async function invokeCmd<T>(cmd: string, payload?: unknown): Promise<T> {
  try {
    if (payload === undefined) {
      return await invoke<T>(cmd);
    }
    return await invoke<T>(cmd, { payload });
  } catch (err) {
    throw new ApiError(parseError(err));
  }
}
