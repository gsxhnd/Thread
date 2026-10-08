/** 集合（文件夹）与请求 IPC 封装。 */
import { invokeCmd } from "./client";
import type {
  AuthSpec,
  CollectionDetail,
  KeyValue,
  Protocol,
  RequestDetail,
  RequestDocument,
  TrashResult,
} from "./types";

export function collectionCreate(input: {
  parentPath?: string | null;
  name: string;
  description?: string;
}): Promise<CollectionDetail> {
  return invokeCmd("collection_create", input);
}

export function collectionGet(path: string): Promise<CollectionDetail> {
  return invokeCmd("collection_get", { path });
}

export function collectionUpdate(input: {
  path: string;
  name?: string;
  description?: string;
  auth?: AuthSpec;
  headers?: KeyValue[];
  preScript?: string;
  postScript?: string;
}): Promise<CollectionDetail> {
  return invokeCmd("collection_update", input);
}

export function collectionDelete(path: string): Promise<TrashResult> {
  return invokeCmd("collection_delete", { path });
}

export function requestCreate(input: {
  collectionPath: string;
  name: string;
  protocol: Protocol;
}): Promise<RequestDetail> {
  return invokeCmd("request_create", input);
}

export function requestGet(path: string): Promise<RequestDetail> {
  return invokeCmd("request_get", { path });
}

export function requestUpdate(input: {
  path: string;
  name: string;
}): Promise<RequestDetail> {
  return invokeCmd("request_update", input);
}

export function requestUpdateConfig(input: {
  path: string;
  document: RequestDocument;
}): Promise<RequestDetail> {
  return invokeCmd("request_update_config", input);
}

export function requestDelete(path: string): Promise<TrashResult> {
  return invokeCmd("request_delete", { path });
}

export function requestDuplicate(input: {
  path: string;
  name?: string;
}): Promise<RequestDetail> {
  return invokeCmd("request_duplicate", input);
}
