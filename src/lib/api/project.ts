import { invokeCmd } from "./client";
import type {
  Chapter,
  ChapterSaveResult,
  ChapterSummary,
  Project,
  Volume,
  VolumeWithChapters,
} from "./types";

export function projectCreate(input: {
  title: string;
  synopsis?: string;
  author?: string;
}): Promise<Project> {
  return invokeCmd("project_create", input);
}

export function projectUpdate(input: {
  id: string;
  title?: string;
  synopsis?: string | null;
  author?: string | null;
}): Promise<Project> {
  return invokeCmd("project_update", input);
}

export function projectDelete(id: string): Promise<void> {
  return invokeCmd("project_delete", { id });
}

export function projectList(): Promise<Project[]> {
  return invokeCmd("project_list", {});
}

export function projectGet(id: string): Promise<Project> {
  return invokeCmd("project_get", { id });
}

export function volumeCreate(input: {
  projectId: string;
  title: string;
}): Promise<Volume> {
  return invokeCmd("volume_create", input);
}

export function volumeUpdate(input: {
  id: string;
  title?: string;
}): Promise<Volume> {
  return invokeCmd("volume_update", input);
}

export function volumeDelete(id: string): Promise<void> {
  return invokeCmd("volume_delete", { id });
}

export function volumeReorder(input: {
  projectId: string;
  orderedIds: string[];
}): Promise<void> {
  return invokeCmd("volume_reorder", input);
}

export function chapterCreate(input: {
  volumeId: string;
  title: string;
}): Promise<Chapter> {
  return invokeCmd("chapter_create", input);
}

export function chapterGet(id: string): Promise<Chapter> {
  return invokeCmd("chapter_get", { id });
}

export function chapterSave(input: {
  id: string;
  content: string;
  title?: string;
}): Promise<ChapterSaveResult> {
  return invokeCmd("chapter_save", input);
}

export function chapterUpdateStatus(input: {
  id: string;
  status: string;
}): Promise<Chapter> {
  return invokeCmd("chapter_update_status", input);
}

export function chapterDelete(id: string): Promise<void> {
  return invokeCmd("chapter_delete", { id });
}

export function chapterReorder(input: {
  volumeId: string;
  orderedIds: string[];
}): Promise<void> {
  return invokeCmd("chapter_reorder", input);
}

export function chapterTree(
  projectId: string,
): Promise<VolumeWithChapters[]> {
  return invokeCmd("chapter_tree", { projectId });
}

export type { ChapterSummary };
