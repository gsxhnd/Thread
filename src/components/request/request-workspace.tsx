import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FilePlus2Icon, FolderPlusIcon, SendIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { collectionCreate, requestCreate, requestGet, type ProtoServiceInfo, type RequestDetail } from "@/lib/api";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { useAppStore } from "@/stores/app-store";
import { useTabsStore, type RequestTab } from "@/stores/tabs-store";
import { useUiStore } from "@/stores/ui-store";
import { ResponsePanel } from "@/components/response/response-panel";
import { RequestEditor } from "./request-editor";
import { GraphqlEditor } from "./graphql-editor";
import { WebsocketEditor } from "./websocket-editor";
import { GrpcEditor } from "./grpc-editor";
import { SessionLog } from "./session-log";
import { RequestActionBar, RequestTabs } from "./request-tabs";
import { useSaveRequest } from "./use-save-request";
import { docString, findProtoMethod, graphqlOperation } from "@/lib/protocol";
import { useSessionStore } from "@/stores/session-store";

/** 主工作区：请求标签页 + 编辑面板 + 响应面板。 */
export function RequestWorkspace() {
  const tabs = useTabsStore((state) => state.tabs);
  const activeKey = useTabsStore((state) => state.activeKey);
  const [editorTab, setEditorTab] = useState("params");
  const save = useSaveRequest();
  const activeTab = tabs.find((tab) => tab.key === activeKey) ?? null;
  const liveServices = useSessionStore((state) =>
    activeKey ? state.byTab[activeKey]?.services : undefined,
  );

  const { data: detail } = useQuery({
    queryKey: ["request-detail", activeTab?.path],
    queryFn: () => requestGet(activeTab?.path as string),
    enabled: !!activeTab?.path,
    // 外部编辑（Git 切换分支、手动改文件）后工作区刷新会失效该查询。
    staleTime: 2_000,
  });

  // Ctrl/Cmd+S 保存当前标签页。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (activeKey) void save(activeKey);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activeKey, save]);

  if (!activeTab) {
    return <EmptyWorkspace />;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <RequestTabs />
      <RequestActionBar tab={activeTab} />
      <ResizablePanelGroup orientation="vertical" className="min-h-0 flex-1">
        <ResizablePanel defaultSize={55} minSize={20}>
          <ProtocolEditor
            tab={activeTab}
            detail={detail ?? null}
            editorTab={editorTab}
            onEditorTabChange={setEditorTab}
          />
        </ResizablePanel>
        <ResizableHandle className="h-1.5 bg-border transition-colors hover:bg-primary/40" />
        <ResizablePanel defaultSize={45} minSize={15}>
          {usesLiveLog(activeTab, liveServices ?? []) ? (
            <SessionLog key={activeTab.key} tab={activeTab} />
          ) : (
            <ResponsePanel tabKey={activeTab.key} />
          )}
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}

function ProtocolEditor({
  tab,
  detail,
  editorTab,
  onEditorTabChange,
}: {
  tab: RequestTab;
  detail: RequestDetail | null;
  editorTab: string;
  onEditorTabChange: (value: string) => void;
}) {
  if (tab.protocol === "graphql") return <GraphqlEditor key={tab.key} tab={tab} detail={detail} />;
  if (tab.protocol === "websocket") return <WebsocketEditor key={tab.key} tab={tab} />;
  if (tab.protocol === "grpc") return <GrpcEditor key={tab.key} tab={tab} />;
  return (
    <RequestEditor
      tab={tab}
      detail={detail}
      activeTab={editorTab}
      onActiveTabChange={onEditorTabChange}
    />
  );
}

function usesLiveLog(tab: RequestTab, services: ProtoServiceInfo[]) {
  if (tab.protocol === "websocket") return true;
  if (tab.protocol === "graphql") {
    return graphqlOperation(docString(tab.document, "query")) === "subscription";
  }
  if (tab.protocol === "grpc") {
    const method = findProtoMethod(
      services,
      docString(tab.document, "service"),
      docString(tab.document, "method"),
    );
    return !!method && (method.clientStreaming || method.serverStreaming);
  }
  return false;
}

function EmptyWorkspace() {
  const snapshot = useAppStore((state) => state.snapshot);
  const selectedCollectionPath = useUiStore((state) => state.selectedCollectionPath);
  const refresh = useAppStore((state) => state.refresh);

  const hasCollections = (snapshot?.index.collections.length ?? 0) > 0;

  const handleNewCollection = async () => {
    await collectionCreate({ name: "New Collection" });
    await refresh();
  };

  const handleNewRequest = async () => {
    if (!selectedCollectionPath) return;
    const created = await requestCreate({
      collectionPath: selectedCollectionPath,
      name: "New Request",
      protocol: "http",
    });
    await refresh();
    useTabsStore.getState().openRequest({
      path: created.path,
      name: created.name,
      protocol: created.protocol,
      collectionPath: created.collectionPath,
      document: created.raw,
    });
  };

  return (
    <div className="flex min-h-0 flex-1 items-center justify-center p-8">
      <div className="flex max-w-md flex-col items-center gap-4 text-center">
        <span className="flex size-12 items-center justify-center rounded-2xl bg-accent text-accent-foreground">
          <SendIcon className="size-5" />
        </span>
        <div className="flex flex-col gap-1.5">
          <h2 className="text-base font-medium">开始一个请求</h2>
          <p className="text-sm leading-relaxed text-muted-foreground">
            集合与请求以 JSON 文件保存在工作区，可直接用 Git 管理。环境变量通过 {"{{variable}}"}{" "}
            在发送前替换。
          </p>
        </div>
        <div className="flex flex-wrap justify-center gap-2">
          <Button variant="outline" size="sm" onClick={() => void handleNewCollection()}>
            <FolderPlusIcon data-icon="inline-start" />
            新建集合
          </Button>
          <Button
            size="sm"
            disabled={!hasCollections || !selectedCollectionPath}
            title={selectedCollectionPath ? undefined : "先在左侧点击一个集合，再新建请求"}
            onClick={() => void handleNewRequest()}
          >
            <FilePlus2Icon data-icon="inline-start" />
            新建请求
          </Button>
        </div>
      </div>
    </div>
  );
}
