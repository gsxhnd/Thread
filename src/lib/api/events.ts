import { listenWithCleanup } from "@/lib/tauri-listener";

export function onChapterSaved(
  cb: (p: { chapterId: string; savedAt: number; wordCount: number }) => void,
): () => void {
  return listenWithCleanup("chapter://saved", (e) =>
    cb(e.payload as { chapterId: string; savedAt: number; wordCount: number }),
  );
}

export function onAiChunk(
  cb: (p: { requestId: string; delta: string }) => void,
): () => void {
  return listenWithCleanup("ai://chunk", (e) =>
    cb(e.payload as { requestId: string; delta: string }),
  );
}

export function onAiDone(
  cb: (p: { requestId: string; finishReason: string }) => void,
): () => void {
  return listenWithCleanup("ai://done", (e) =>
    cb(e.payload as { requestId: string; finishReason: string }),
  );
}

export function onAiError(
  cb: (p: { requestId: string; code: string; message: string }) => void,
): () => void {
  return listenWithCleanup("ai://error", (e) =>
    cb(e.payload as { requestId: string; code: string; message: string }),
  );
}

export function onExportProgress(
  cb: (p: { taskId: string; percent: number }) => void,
): () => void {
  return listenWithCleanup("export://progress", (e) =>
    cb(e.payload as { taskId: string; percent: number }),
  );
}
