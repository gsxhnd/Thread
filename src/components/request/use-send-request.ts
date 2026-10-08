import { useCallback } from "react";
import {
  activeEnvironmentPath,
  useAppStore,
} from "@/stores/app-store";
import { useResponseStore } from "@/stores/response-store";
import { useTabsStore } from "@/stores/tabs-store";
import { httpCancel, httpSend, newTaskId } from "@/lib/api";
import type { HttpResponse } from "@/lib/api";
import { toHttpConfig } from "@/lib/request-config";

/**
 * 发送 / 取消请求的共享逻辑。
 *
 * 请求在 Rust 侧执行（无 CORS 限制、可控制 TLS/重定向/超时）。
 * taskId 由前端生成，因此请求在飞行中即可取消。
 */
export function useSendRequest() {
  const send = useCallback(
    async (tabKey: string): Promise<HttpResponse | null> => {
      const tab = useTabsStore.getState().tabs.find((item) => item.key === tabKey);
      if (!tab) return null;

      const taskId = newTaskId();
      const store = useResponseStore.getState();
      store.start(tabKey, taskId);

      try {
        const response = await httpSend({
          taskId,
          requestPath: tab.path,
          config: toHttpConfig(tab.document),
          environmentPath: activeEnvironmentPath(useAppStore.getState().snapshot),
        });
        useResponseStore.getState().succeed(tabKey, response);
        return response;
      } catch (error) {
        useResponseStore.getState().fail(tabKey, error);
        return null;
      }
    },
    [],
  );

  const cancel = useCallback(async (taskId: string) => {
    await httpCancel(taskId);
  }, []);

  return { send, cancel };
}
