import { create } from "zustand";
import type { Project } from "@/lib/api/types";

interface ProjectState {
  currentProjectId: string | null;
  projects: Project[];
  setProjects: (projects: Project[]) => void;
  setCurrentProjectId: (id: string | null) => void;
}

export const useProjectStore = create<ProjectState>((set) => ({
  currentProjectId: null,
  projects: [],
  setProjects: (projects) => set({ projects }),
  setCurrentProjectId: (id) => set({ currentProjectId: id }),
}));
