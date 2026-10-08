import { useCallback } from "react";
import { toast } from "sonner";
import { requestCreate, requestUpdateConfig } from "@/lib/api";
import { useAppStore } from "@/stores/app-store";
import { useTabsStore } from "@/stores/tabs-store";

/** 把标签页草稿写回工作区文件（未保存的草稿会先创建文件）。 */
export function useSaveRequest() {
  const refresh = useAppStore((state) => state.refresh);

  return useCallback(
    async (tabKey: string) => {
      const tab = useTabsStore.getState().tabs.find((item) => item.key === tabKey);
      if (!tab) return;
      if (!tab.dirty && tab.path) {
        return;
      }

      try {
        if (!tab.path) {
          if (!tab.collectionPath) {
            toast.error("请先在左侧选择一个集合，再保存草稿");
            return;
          }
          const created = await requestCreate({
            collectionPath: tab.collectionPath,
            name: tab.name,
            protocol: "http",
          });
          await requestUpdateConfig({ path: created.path, document: tab.document });
          useTabsStore.getState().markSaved(tab.key, created.path, tab.name);
        } else {
          await requestUpdateConfig({ path: tab.path, document: tab.document });
          useTabsStore.getState().markSaved(tab.key, tab.path, tab.name);
        }
        await refresh();
        toast.success("已保存到工作区文件");
      } catch (error) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    },
    [refresh],
  );
}
