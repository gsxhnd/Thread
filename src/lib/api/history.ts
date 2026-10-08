/** 请求历史 IPC 封装（本地数据，不进 Git）。 */
import { invokeCmd } from "./client";
import type { HistoryDetail, HistoryPage, RequestDetail } from "./types";

export function historyList(input?: {
  protocol?: string;
  limit?: number;
  offset?: number;
}): Promise<HistoryPage> {
  return invokeCmd("history_list", input ?? {});
}

export function historyGet(id: string): Promise<HistoryDetail> {
  return invokeCmd("history_get", { id });
}

export function historyDelete(id: string): Promise<void> {
  return invokeCmd("history_delete", { id });
}

export function historyClear(
  beforeTimestamp?: number,
): Promise<{ deletedCount: number }> {
  return invokeCmd("history_clear", { beforeTimestamp });
}

export function historySaveToCollection(input: {
  id: string;
  collectionPath: string;
  name?: string;
}): Promise<RequestDetail> {
  return invokeCmd("history_save_to_collection", input);
}
