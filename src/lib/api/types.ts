/** Shared domain types matching Rust IPC models (camelCase). */

export interface AppError {
  code: string;
  message: string;
}

export interface Project {
  id: string;
  title: string;
  synopsis?: string | null;
  author?: string | null;
  coverPath?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Volume {
  id: string;
  projectId: string;
  title: string;
  sortOrder: number;
  createdAt: number;
  updatedAt: number;
}

export interface Chapter {
  id: string;
  volumeId: string;
  projectId: string;
  title: string;
  content: string;
  summary?: string | null;
  wordCount: number;
  status: "draft" | "done" | string;
  sortOrder: number;
  createdAt: number;
  updatedAt: number;
}

export interface ChapterSummary {
  id: string;
  volumeId: string;
  projectId: string;
  title: string;
  summary?: string | null;
  wordCount: number;
  status: string;
  sortOrder: number;
  updatedAt: number;
}

export interface VolumeWithChapters extends Volume {
  chapters: ChapterSummary[];
}

export interface Plotline {
  id: string;
  projectId: string;
  name: string;
  color?: string | null;
  description?: string | null;
  sortOrder: number;
  createdAt: number;
  updatedAt: number;
}

export interface OutlineNode {
  chapterId: string;
  chapterTitle: string;
  volumeId: string;
  volumeTitle: string;
  summary?: string | null;
  sortOrder: number;
  plotlines: Plotline[];
}

export interface Character {
  id: string;
  projectId: string;
  name: string;
  alias?: string | null;
  avatarPath?: string | null;
  bio?: string | null;
  attributes?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface CharacterRelation {
  id: string;
  projectId: string;
  fromCharId: string;
  toCharId: string;
  relationType: string;
  description?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface RelationGraph {
  nodes: Character[];
  edges: CharacterRelation[];
}

export interface Location {
  id: string;
  projectId: string;
  name: string;
  description?: string | null;
  parentId?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Faction {
  id: string;
  projectId: string;
  name: string;
  description?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Item {
  id: string;
  projectId: string;
  name: string;
  description?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface WorldEntry {
  id: string;
  entryType: string;
  name: string;
  description?: string | null;
  parentId?: string | null;
  updatedAt: number;
}

export interface EventLink {
  id: string;
  eventId: string;
  entityType: string;
  entityId: string;
  createdAt: number;
}

export interface TimelineEvent {
  id: string;
  projectId: string;
  title: string;
  description?: string | null;
  storyTime?: string | null;
  sortKey: number;
  createdAt: number;
  updatedAt: number;
  links: EventLink[];
}

export interface Foreshadowing {
  id: string;
  projectId: string;
  title: string;
  description?: string | null;
  status: "planted" | "resolved" | string;
  setupChapterId?: string | null;
  payoffChapterId?: string | null;
  importance: number;
  createdAt: number;
  updatedAt: number;
}

export interface ChapterSaveResult {
  savedAt: number;
  wordCount: number;
}

export interface StatsOverview {
  totalWords: number;
  chapterCount: number;
  volumeCount: number;
  characterCount: number;
  unresolvedForeshadowings: number;
  wordsToday: number;
}

export interface DayCount {
  date: string;
  words: number;
}

export interface Streak {
  current: number;
  longest: number;
}

export interface Goal {
  projectId: string;
  dailyWords?: number | null;
  totalWords?: number | null;
  updatedAt: number;
}

export interface AiConfigPublic {
  baseUrl: string;
  model: string;
  hasKey: boolean;
}

export type AiMode = "continue" | "polish" | "inspire" | "ask";

export type ExportFormat = "txt" | "md" | "epub" | "docx";
