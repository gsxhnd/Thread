import { create } from "zustand";
import {
  workspaceClose,
  workspaceCreate,
  workspaceCurrent,
  workspaceInit,
  workspaceOpen,
  workspaceRefresh,
} from "@/lib/api";
import type { WorkspaceSnapshot } from "@/lib/api";

type OpenMode = "open" | "create" | "init";

interface AppState {
  snapshot: WorkspaceSnapshot | null;
  loading: boolean;
  error: string | null;
  /** 应用启动时恢复上次打开的工作区。 */
  bootstrap: () => Promise<void>;
  openWorkspace: (path: string, name?: string, mode?: OpenMode) => Promise<void>;
  refresh: () => Promise<void>;
  closeWorkspace: () => Promise<void>;
  setSnapshot: (snapshot: WorkspaceSnapshot) => void;
  clearError: () => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  snapshot: null,
  loading: false,
  error: null,

  bootstrap: async () => {
    set({ loading: true });
    try {
      // Rust 侧启动时已尝试恢复上次工作区，这里只读取当前状态。
      const snapshot = await workspaceCurrent();
      set({ snapshot, loading: false, error: null });
    } catch (error) {
      set({
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  },

  openWorkspace: async (path, name, mode = "open") => {
    set({ loading: true, error: null });
    try {
      const trimmed = path.trim();
      const snapshot =
        mode === "create"
          ? await workspaceCreate({ path: trimmed, name })
          : mode === "init"
            ? await workspaceInit({ path: trimmed, name })
            : await workspaceOpen({ path: trimmed });
      set({ snapshot, loading: false, error: null });
    } catch (error) {
      set({
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  },

  refresh: async () => {
    if (!get().snapshot) return;
    try {
      const snapshot = await workspaceRefresh();
      set({ snapshot, error: null });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },

  closeWorkspace: async () => {
    await workspaceClose();
    set({ snapshot: null, error: null });
  },

  setSnapshot: (snapshot) => set({ snapshot }),
  clearError: () => set({ error: null }),
}));

/** 当前激活环境的相对路径（工作区配置优先，索引兜底）。 */
export function activeEnvironmentPath(
  snapshot: WorkspaceSnapshot | null,
): string | null {
  if (!snapshot) return null;
  const configured = snapshot.config.activeEnvironment;
  if (configured) {
    const matched = snapshot.index.environments.find(
      (environment) => environment.path === configured,
    );
    if (matched) return matched.path;
  }
  return (
    snapshot.index.environments.find((environment) => environment.isActive)?.path ??
    null
  );
}
