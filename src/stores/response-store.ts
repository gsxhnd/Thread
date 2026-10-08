import { create } from "zustand";
import type { HttpResponse } from "@/lib/api";
import { isApiError } from "@/lib/api";

export type ResponseState =
  | { status: "idle" }
  | { status: "loading"; taskId: string; startedAt: number }
  | { status: "success"; response: HttpResponse; receivedAt: number }
  | { status: "error"; code: string; message: string };

interface ResponseStoreState {
  /** 按标签页 key 保存最近一次响应，切换标签页不会丢结果。 */
  byTab: Record<string, ResponseState>;
  start: (key: string, taskId: string) => void;
  succeed: (key: string, response: HttpResponse) => void;
  fail: (key: string, error: unknown) => void;
  clear: (key: string) => void;
  get: (key: string) => ResponseState;
}

export const useResponseStore = create<ResponseStoreState>((set, get) => ({
  byTab: {},

  start: (key, taskId) =>
    set((state) => ({
      byTab: {
        ...state.byTab,
        [key]: { status: "loading", taskId, startedAt: Date.now() },
      },
    })),

  succeed: (key, response) =>
    set((state) => ({
      byTab: {
        ...state.byTab,
        [key]: { status: "success", response, receivedAt: Date.now() },
      },
    })),

  fail: (key, error) =>
    set((state) => ({
      byTab: {
        ...state.byTab,
        [key]: isApiError(error)
          ? { status: "error", code: error.code, message: error.message }
          : {
              status: "error",
              code: "UNKNOWN",
              message: error instanceof Error ? error.message : String(error),
            },
      },
    })),

  clear: (key) =>
    set((state) => ({ byTab: { ...state.byTab, [key]: { status: "idle" } } })),

  get: (key) => get().byTab[key] ?? { status: "idle" },
}));
