import { create } from "zustand";

export type RightPanelTab = "context" | "foreshadowing" | "ai";

interface UiState {
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  focusMode: boolean;
  rightTab: RightPanelTab;
  setLeftCollapsed: (v: boolean) => void;
  setRightCollapsed: (v: boolean) => void;
  setFocusMode: (v: boolean) => void;
  setRightTab: (tab: RightPanelTab) => void;
  toggleFocusMode: () => void;
}

export const useUiStore = create<UiState>((set, get) => ({
  leftCollapsed: false,
  rightCollapsed: false,
  focusMode: false,
  rightTab: "context",
  setLeftCollapsed: (v) => set({ leftCollapsed: v }),
  setRightCollapsed: (v) => set({ rightCollapsed: v }),
  setFocusMode: (v) =>
    set({
      focusMode: v,
      leftCollapsed: v,
      rightCollapsed: v,
    }),
  setRightTab: (tab) => set({ rightTab: tab }),
  toggleFocusMode: () => get().setFocusMode(!get().focusMode),
}));
