import { create } from "zustand";

export type RightPanelTab = "context" | "foreshadowing" | "ai";

type PanelSnapshot = {
  navCollapsed: boolean;
  leftCollapsed: boolean;
  rightCollapsed: boolean;
};

interface UiState {
  navCollapsed: boolean;
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  focusMode: boolean;
  rightTab: RightPanelTab;
  /** Volume ids that are collapsed in the chapter tree. */
  collapsedVolumeIds: Record<string, true>;
  preFocus: PanelSnapshot | null;
  setNavCollapsed: (v: boolean) => void;
  setLeftCollapsed: (v: boolean) => void;
  setRightCollapsed: (v: boolean) => void;
  setFocusMode: (v: boolean) => void;
  setRightTab: (tab: RightPanelTab) => void;
  toggleFocusMode: () => void;
  toggleVolumeCollapsed: (volumeId: string) => void;
}

export const useUiStore = create<UiState>((set, get) => ({
  navCollapsed: false,
  leftCollapsed: false,
  rightCollapsed: false,
  focusMode: false,
  rightTab: "foreshadowing",
  collapsedVolumeIds: {},
  preFocus: null,
  setNavCollapsed: (v) =>
    set((state) => ({
      navCollapsed: v,
      focusMode: state.focusMode && v ? state.focusMode : false,
    })),
  setLeftCollapsed: (v) =>
    set((state) => ({
      leftCollapsed: v,
      // Manually expanding a panel exits focus mode.
      focusMode: state.focusMode && !v ? false : state.focusMode,
    })),
  setRightCollapsed: (v) =>
    set((state) => ({
      rightCollapsed: v,
      focusMode: state.focusMode && !v ? false : state.focusMode,
    })),
  setFocusMode: (v) => {
    const state = get();
    if (v) {
      set({
        focusMode: true,
        preFocus: {
          navCollapsed: state.navCollapsed,
          leftCollapsed: state.leftCollapsed,
          rightCollapsed: state.rightCollapsed,
        },
        navCollapsed: true,
        leftCollapsed: true,
        rightCollapsed: true,
      });
      return;
    }
    const pre = state.preFocus;
    set({
      focusMode: false,
      preFocus: null,
      navCollapsed: pre?.navCollapsed ?? false,
      leftCollapsed: pre?.leftCollapsed ?? false,
      rightCollapsed: pre?.rightCollapsed ?? false,
    });
  },
  setRightTab: (tab) => set({ rightTab: tab }),
  toggleFocusMode: () => get().setFocusMode(!get().focusMode),
  toggleVolumeCollapsed: (volumeId) =>
    set((state) => {
      const next = { ...state.collapsedVolumeIds };
      if (next[volumeId]) {
        delete next[volumeId];
      } else {
        next[volumeId] = true;
      }
      return { collapsedVolumeIds: next };
    }),
}));
