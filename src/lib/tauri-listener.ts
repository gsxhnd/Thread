/**
 * Tauri event listener with safe async setup/cleanup.
 *
 * 封装 Tauri `listen` 的异步注册竞态：若在 `listen` resolve 前已卸载，
 * 立即 dispose；否则保存 unlisten 供后续清理。
 */
import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";

export function listenWithCleanup<T>(
  event: string,
  handler: (event: Event<T>) => void,
): () => void {
  let disposed = false;
  let unlisten: UnlistenFn | undefined;

  void listen<T>(event, handler).then((dispose) => {
    if (disposed) {
      dispose();
      return;
    }
    unlisten = dispose;
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}
