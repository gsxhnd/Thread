import { invokeCmd } from "./client";
import type {
  Character,
  CharacterRelation,
  OutlineNode,
  Plotline,
  RelationGraph,
} from "./types";

export function plotlineCreate(input: {
  projectId: string;
  name: string;
  color?: string;
}): Promise<Plotline> {
  return invokeCmd("plotline_create", input);
}

export function plotlineUpdate(input: {
  id: string;
  name?: string;
  color?: string | null;
  description?: string | null;
}): Promise<Plotline> {
  return invokeCmd("plotline_update", input);
}

export function plotlineDelete(id: string): Promise<void> {
  return invokeCmd("plotline_delete", { id });
}

export function plotlineAttach(input: {
  chapterId: string;
  plotlineId: string;
}): Promise<void> {
  return invokeCmd("plotline_attach", input);
}

export function plotlineDetach(input: {
  chapterId: string;
  plotlineId: string;
}): Promise<void> {
  return invokeCmd("plotline_detach", input);
}

export function outlineList(projectId: string): Promise<OutlineNode[]> {
  return invokeCmd("outline_list", { projectId });
}

export function characterCreate(input: {
  projectId: string;
  name: string;
  alias?: string;
  bio?: string;
  attributes?: string;
}): Promise<Character> {
  return invokeCmd("character_create", input);
}

export function characterUpdate(input: {
  id: string;
  name?: string;
  alias?: string | null;
  bio?: string | null;
  attributes?: string | null;
}): Promise<Character> {
  return invokeCmd("character_update", input);
}

export function characterDelete(id: string): Promise<void> {
  return invokeCmd("character_delete", { id });
}

export function characterList(projectId: string): Promise<Character[]> {
  return invokeCmd("character_list", { projectId });
}

export function relationCreate(input: {
  projectId: string;
  fromCharId: string;
  toCharId: string;
  relationType: string;
  description?: string;
}): Promise<CharacterRelation> {
  return invokeCmd("relation_create", input);
}

export function relationDelete(id: string): Promise<void> {
  return invokeCmd("relation_delete", { id });
}

export function relationGraph(projectId: string): Promise<RelationGraph> {
  return invokeCmd("relation_graph", { projectId });
}
