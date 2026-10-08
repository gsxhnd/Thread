import { create } from "zustand";
import { loadConfigSections, setConfigSection } from "@/lib/api";

export type WorkspaceView = "request" | "history" | "environments" | "settings";

const LAYOUT_SECTION = "ui_layout";

type PersistedLayout = {
  view?: WorkspaceView;
  sidebarCollapsed?: boolean;
};

/** UI 布局写入 config.toml 的 ui_layout 分区（失败不影响使用）。 */
function persistLayout(layout: PersistedLayout) {
  void setConfigSection(LAYOUT_SECTION, JSON.stringify(layout)).catch(() => {
    /* 布局持久化失败不阻塞交互 */
  });
}

interface UiState {
  view: WorkspaceView;
  /** 侧边栏中选中的集合路径（用于新建请求的默认位置）。 */
  selectedCollectionPath: string | null;
  sidebarCollapsed: boolean;
  search: string;
  setView: (view: WorkspaceView) => void;
  selectCollection: (path: string | null) => void;
  toggleSidebar: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSearch: (search: string) => void;
  /** 启动时恢复上次的布局（config.toml: ui_layout）。 */
  hydrate: () => Promise<void>;
  reset: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  view: "request",
  selectedCollectionPath: null,
  sidebarCollapsed: false,
  search: "",
  setView: (view) => {
    set({ view });
    persistLayout({
      view,
      sidebarCollapsed: useUiStore.getState().sidebarCollapsed,
    });
  },
  selectCollection: (path) => set({ selectedCollectionPath: path }),
  toggleSidebar: () => {
    const sidebarCollapsed = !useUiStore.getState().sidebarCollapsed;
    set({ sidebarCollapsed });
    persistLayout({ view: useUiStore.getState().view, sidebarCollapsed });
  },
  setSidebarCollapsed: (collapsed) => {
    set({ sidebarCollapsed: collapsed });
    persistLayout({ view: useUiStore.getState().view, sidebarCollapsed: collapsed });
  },
  setSearch: (search) => set({ search }),

  hydrate: async () => {
    try {
      const sections = await loadConfigSections();
      const raw = sections[LAYOUT_SECTION];
      if (!raw) return;
      const layout = JSON.parse(raw) as PersistedLayout;
      set({
        view: layout.view ?? "request",
        sidebarCollapsed: layout.sidebarCollapsed ?? false,
      });
    } catch {
      /* 首次启动或配置损坏时保持默认布局 */
    }
  },
  reset: () => set({ view: "request", selectedCollectionPath: null, search: "" }),
}));
