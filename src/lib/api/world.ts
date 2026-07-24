import { invokeCmd } from "./client";
import type {
  DayCount,
  Faction,
  Foreshadowing,
  Goal,
  Item,
  Location,
  StatsOverview,
  Streak,
  TimelineEvent,
  WorldEntry,
} from "./types";

export function locationCreate(input: {
  projectId: string;
  name: string;
  description?: string;
  parentId?: string;
}): Promise<Location> {
  return invokeCmd("location_create", input);
}

export function locationUpdate(input: {
  id: string;
  name?: string;
  description?: string | null;
  parentId?: string | null;
}): Promise<Location> {
  return invokeCmd("location_update", input);
}

export function locationDelete(id: string): Promise<void> {
  return invokeCmd("location_delete", { id });
}

export function factionCreate(input: {
  projectId: string;
  name: string;
  description?: string;
}): Promise<Faction> {
  return invokeCmd("faction_create", input);
}

export function factionUpdate(input: {
  id: string;
  name?: string;
  description?: string | null;
}): Promise<Faction> {
  return invokeCmd("faction_update", input);
}

export function factionDelete(id: string): Promise<void> {
  return invokeCmd("faction_delete", { id });
}

export function itemCreate(input: {
  projectId: string;
  name: string;
  description?: string;
}): Promise<Item> {
  return invokeCmd("item_create", input);
}

export function itemUpdate(input: {
  id: string;
  name?: string;
  description?: string | null;
}): Promise<Item> {
  return invokeCmd("item_update", input);
}

export function itemDelete(id: string): Promise<void> {
  return invokeCmd("item_delete", { id });
}

export function worldbuildingList(
  projectId: string,
  type?: string,
): Promise<WorldEntry[]> {
  return invokeCmd("worldbuilding_list", { projectId, type });
}

export function eventCreate(input: {
  projectId: string;
  title: string;
  storyTime?: string;
  description?: string;
}): Promise<TimelineEvent> {
  return invokeCmd("event_create", input);
}

export function eventUpdate(input: {
  id: string;
  title?: string;
  storyTime?: string | null;
  description?: string | null;
}): Promise<TimelineEvent> {
  return invokeCmd("event_update", input);
}

export function eventDelete(id: string): Promise<void> {
  return invokeCmd("event_delete", { id });
}

export function eventReorder(input: {
  projectId: string;
  orderedIds: string[];
}): Promise<void> {
  return invokeCmd("event_reorder", input);
}

export function eventLink(input: {
  eventId: string;
  entityType: string;
  entityId: string;
}): Promise<void> {
  return invokeCmd("event_link", input);
}

export function eventUnlink(input: {
  eventId: string;
  entityType: string;
  entityId: string;
}): Promise<void> {
  return invokeCmd("event_unlink", input);
}

export function timelineList(projectId: string): Promise<TimelineEvent[]> {
  return invokeCmd("timeline_list", { projectId });
}

export function foreshadowingCreate(input: {
  projectId: string;
  title: string;
  description?: string;
  importance?: number;
}): Promise<Foreshadowing> {
  return invokeCmd("foreshadowing_create", input);
}

export function foreshadowingUpdate(input: {
  id: string;
  title?: string;
  description?: string | null;
  importance?: number;
}): Promise<Foreshadowing> {
  return invokeCmd("foreshadowing_update", input);
}

export function foreshadowingDelete(id: string): Promise<void> {
  return invokeCmd("foreshadowing_delete", { id });
}

export function foreshadowingLink(input: {
  id: string;
  setupChapterId?: string | null;
  payoffChapterId?: string | null;
}): Promise<{ status: string }> {
  return invokeCmd("foreshadowing_link", input);
}

export function foreshadowingList(
  projectId: string,
  status?: string,
): Promise<Foreshadowing[]> {
  return invokeCmd("foreshadowing_list", { projectId, status });
}

export function foreshadowingUnresolved(
  projectId: string,
): Promise<Foreshadowing[]> {
  return invokeCmd("foreshadowing_unresolved", { projectId });
}

export function statsOverview(projectId: string): Promise<StatsOverview> {
  return invokeCmd("stats_overview", { projectId });
}

export function statsCalendar(
  projectId: string,
  year: number,
): Promise<DayCount[]> {
  return invokeCmd("stats_calendar", { projectId, year });
}

export function statsStreak(projectId: string): Promise<Streak> {
  return invokeCmd("stats_streak", { projectId });
}

export function goalSet(input: {
  projectId: string;
  dailyWords?: number;
  totalWords?: number;
}): Promise<Goal> {
  return invokeCmd("goal_set", input);
}

export function goalGet(projectId: string): Promise<Goal | null> {
  return invokeCmd("goal_get", { projectId });
}
