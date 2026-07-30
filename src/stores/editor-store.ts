import { create } from "zustand";

export type SaveState = "idle" | "saving" | "saved" | "error";

interface EditorState {
  chapterId: string | null;
  draft: string;
  title: string;
  dirty: boolean;
  saveState: SaveState;
  wordCount: number;
  setChapter: (id: string, title: string, content: string, wordCount: number) => void;
  setDraft: (md: string) => void;
  setTitle: (title: string) => void;
  setSaving: () => void;
  /** Only clears dirty when `forChapterId` matches the active chapter (or is omitted). */
  markSaved: (wordCount: number, forChapterId?: string) => void;
  markError: () => void;
  clear: () => void;
}

export const useEditorStore = create<EditorState>((set, get) => ({
  chapterId: null,
  draft: "",
  title: "",
  dirty: false,
  saveState: "idle",
  wordCount: 0,
  setChapter: (id, title, content, wordCount) =>
    set({
      chapterId: id,
      title,
      draft: content,
      wordCount,
      dirty: false,
      saveState: "idle",
    }),
  setDraft: (md) => set({ draft: md, dirty: true, saveState: "idle" }),
  setTitle: (title) => set({ title, dirty: true }),
  setSaving: () => set({ saveState: "saving" }),
  markSaved: (wordCount, forChapterId) => {
    if (forChapterId !== undefined && get().chapterId !== forChapterId) return;
    set({ dirty: false, saveState: "saved", wordCount });
  },
  markError: () => set({ saveState: "error" }),
  clear: () =>
    set({
      chapterId: null,
      draft: "",
      title: "",
      dirty: false,
      saveState: "idle",
      wordCount: 0,
    }),
}));
