import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export function onChapterSaved(
  cb: (p: { chapterId: string; savedAt: number; wordCount: number }) => void,
): Promise<UnlistenFn> {
  return listen("chapter://saved", (e) =>
    cb(e.payload as { chapterId: string; savedAt: number; wordCount: number }),
  );
}

export function onAiChunk(
  cb: (p: { requestId: string; delta: string }) => void,
): Promise<UnlistenFn> {
  return listen("ai://chunk", (e) =>
    cb(e.payload as { requestId: string; delta: string }),
  );
}

export function onAiDone(
  cb: (p: { requestId: string; finishReason: string }) => void,
): Promise<UnlistenFn> {
  return listen("ai://done", (e) =>
    cb(e.payload as { requestId: string; finishReason: string }),
  );
}

export function onAiError(
  cb: (p: { requestId: string; code: string; message: string }) => void,
): Promise<UnlistenFn> {
  return listen("ai://error", (e) =>
    cb(e.payload as { requestId: string; code: string; message: string }),
  );
}

export function onExportProgress(
  cb: (p: { taskId: string; percent: number }) => void,
): Promise<UnlistenFn> {
  return listen("export://progress", (e) =>
    cb(e.payload as { taskId: string; percent: number }),
  );
}
