/** 后端事件订阅（docs/04.ipc-api.md §5）。 */
import { listenWithCleanup } from "@/lib/tauri-listener";
import type { WorkspaceChangedEvent, WorkspaceOpenedEvent } from "./types";

/** 外部编辑 / Git 切换分支后，工作区索引发生变化。 */
export function onWorkspaceChanged(
  cb: (payload: WorkspaceChangedEvent) => void,
): () => void {
  return listenWithCleanup<WorkspaceChangedEvent>("workspace://changed", (event) =>
    cb(event.payload),
  );
}

export function onWorkspaceOpened(
  cb: (payload: WorkspaceOpenedEvent) => void,
): () => void {
  return listenWithCleanup<WorkspaceOpenedEvent>("workspace://opened", (event) =>
    cb(event.payload),
  );
}

export function onWorkspaceClosed(cb: () => void): () => void {
  return listenWithCleanup("workspace://closed", () => cb());
}

export function onConfigChanged(cb: (section: string) => void): () => void {
  return listenWithCleanup<string>("thread:config-changed", (event) =>
    cb(event.payload),
  );
}
