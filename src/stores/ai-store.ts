import { create } from "zustand";
import type { AiMode } from "@/lib/api/types";

interface AiState {
  mode: AiMode;
  buffer: string;
  requestId: string | null;
  running: boolean;
  error: string | null;
  setMode: (mode: AiMode) => void;
  start: (requestId: string) => void;
  append: (delta: string) => void;
  finish: () => void;
  fail: (message: string) => void;
  clear: () => void;
}

export const useAiStore = create<AiState>((set) => ({
  mode: "continue",
  buffer: "",
  requestId: null,
  running: false,
  error: null,
  setMode: (mode) => set({ mode }),
  start: (requestId) =>
    set({ requestId, running: true, buffer: "", error: null }),
  append: (delta) => set((s) => ({ buffer: s.buffer + delta })),
  finish: () => set({ running: false }),
  fail: (message) => set({ running: false, error: message }),
  clear: () =>
    set({ buffer: "", requestId: null, running: false, error: null }),
}));
