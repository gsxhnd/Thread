import { create } from "zustand";
import type { RequestDocument } from "@/lib/api";

export interface RequestTab {
  /** 稳定 key：已保存请求用相对路径，未保存草稿用 `draft:<uuid>`。 */
  key: string;
  path: string | null;
  name: string;
  protocol: string;
  collectionPath: string | null;
  /** 工作副本：未保存的编辑都在这里。 */
  document: RequestDocument;
  dirty: boolean;
}

interface TabsState {
  tabs: RequestTab[];
  activeKey: string | null;
  openRequest: (input: {
    path: string;
    name: string;
    protocol: string;
    collectionPath: string;
    document: RequestDocument;
  }) => void;
  openDraft: (input: {
    collectionPath: string;
    name: string;
    protocol?: string;
  }) => void;
  closeTab: (key: string) => void;
  closeOthers: (key: string) => void;
  closeAll: () => void;
  setActive: (key: string) => void;
  updateDocument: (key: string, document: RequestDocument) => void;
  updateName: (key: string, name: string) => void;
  markSaved: (key: string, path: string, name: string) => void;
  renameTab: (key: string, name: string) => void;
  activeTab: () => RequestTab | null;
}

export const useTabsStore = create<TabsState>((set, get) => ({
  tabs: [],
  activeKey: null,

  openRequest: ({ path, name, protocol, collectionPath, document }) => {
    const existing = get().tabs.find((tab) => tab.key === path);
    if (existing) {
      set((state) => ({
        activeKey: path,
        tabs: state.tabs.map((tab) =>
          tab.key === path && !tab.dirty
            ? { ...tab, name, protocol, document }
            : tab,
        ),
      }));
      return;
    }
    const tab: RequestTab = {
      key: path,
      path,
      name,
      protocol,
      collectionPath,
      document,
      dirty: false,
    };
    set((state) => ({ tabs: [...state.tabs, tab], activeKey: path }));
  },

  openDraft: ({ collectionPath, name, protocol = "http" }) => {
    const key = `draft:${crypto.randomUUID()}`;
    const document: RequestDocument = {
      version: 1,
      protocol,
      name,
      method: "GET",
      url: "",
      queryParams: [],
      headers: [],
      body: { type: "none", content: "" },
      auth: { type: "inherit" },
      settings: {},
    };
    const tab: RequestTab = {
      key,
      path: null,
      name,
      protocol,
      collectionPath,
      document,
      dirty: true,
    };
    set((state) => ({ tabs: [...state.tabs, tab], activeKey: key }));
  },

  closeTab: (key) => {
    set((state) => {
      const tabs = state.tabs.filter((tab) => tab.key !== key);
      let activeKey = state.activeKey;
      if (activeKey === key) {
        const index = state.tabs.findIndex((tab) => tab.key === key);
        const fallback = tabs[Math.min(index, tabs.length - 1)];
        activeKey = fallback ? fallback.key : null;
      }
      return { tabs, activeKey };
    });
  },

  closeOthers: (key) =>
    set((state) => ({
      tabs: state.tabs.filter((tab) => tab.key === key),
      activeKey: state.tabs.some((tab) => tab.key === key) ? key : null,
    })),

  closeAll: () => set({ tabs: [], activeKey: null }),

  setActive: (key) => set({ activeKey: key }),

  updateDocument: (key, document) =>
    set((state) => ({
      tabs: state.tabs.map((tab) =>
        tab.key === key
          ? {
              ...tab,
              document,
              dirty: true,
              name:
                typeof document.name === "string" && document.name.trim()
                  ? document.name
                  : tab.name,
            }
          : tab,
      ),
    })),

  updateName: (key, name) =>
    set((state) => ({
      tabs: state.tabs.map((tab) =>
        tab.key === key
          ? { ...tab, name, document: { ...tab.document, name }, dirty: true }
          : tab,
      ),
    })),

  markSaved: (key, path, name) =>
    set((state) => {
      const tabs = state.tabs.map((tab) =>
        tab.key === key
          ? {
              ...tab,
              key: path,
              path,
              name,
              document: { ...tab.document, id: tab.document.id, name },
              dirty: false,
            }
          : tab,
      );
      const activeKey = state.activeKey === key ? path : state.activeKey;
      return { tabs, activeKey };
    }),

  renameTab: (key, name) =>
    set((state) => ({
      tabs: state.tabs.map((tab) => (tab.key === key ? { ...tab, name } : tab)),
    })),

  activeTab: () => {
    const { tabs, activeKey } = get();
    return tabs.find((tab) => tab.key === activeKey) ?? null;
  },
}));
