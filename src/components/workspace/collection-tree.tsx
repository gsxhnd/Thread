import { useMemo, useState } from "react";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  FolderIcon,
  FolderOpenIcon,
  PlusIcon,
  Trash2Icon,
  CopyIcon,
  PencilIcon,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { SelectField } from "@/components/ui/select-field";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  collectionCreate,
  collectionDelete,
  collectionUpdate,
  requestCreate,
  requestDelete,
  requestDuplicate,
  requestGet,
  requestUpdate,
} from "@/lib/api";
import type { CollectionNode, Protocol, RequestNode } from "@/lib/api";
import { methodColor } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app-store";
import { useTabsStore } from "@/stores/tabs-store";
import { useUiStore } from "@/stores/ui-store";
import { PromptDialog } from "./prompt-dialog";

type TreeAction =
  | { kind: "new-request"; collectionPath: string }
  | { kind: "new-collection"; parentPath: string | null }
  | { kind: "rename-collection"; path: string; name: string }
  | { kind: "rename-request"; path: string; name: string };

/** 集合/文件夹与请求的可展开树；支持基本搜索过滤。 */
export function CollectionTree() {
  const snapshot = useAppStore((state) => state.snapshot);
  const refresh = useAppStore((state) => state.refresh);
  const search = useUiStore((state) => state.search);
  const activeKey = useTabsStore((state) => state.activeKey);
  const [collapsed, setCollapsed] = useState<Record<string, true>>({});
  const [action, setAction] = useState<TreeAction | null>(null);

  const collections = snapshot?.index.collections ?? [];
  const needle = search.trim().toLowerCase();

  const filtered = useMemo(
    () => (needle ? filterCollections(collections, needle) : collections),
    [collections, needle],
  );

  const toggle = (path: string) =>
    setCollapsed((state) => {
      const next = { ...state };
      if (next[path]) delete next[path];
      else next[path] = true;
      return next;
    });

  const handleOpenRequest = async (node: RequestNode) => {
    try {
      const detail = await requestGet(node.path);
      useTabsStore.getState().openRequest({
        path: detail.path,
        name: detail.name,
        protocol: detail.protocol,
        collectionPath: detail.collectionPath,
        document: detail.raw,
      });
      useUiStore.getState().setView("request");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const handleDeleteRequest = async (node: RequestNode) => {
    try {
      const result = await requestDelete(node.path);
      useTabsStore.getState().closeTab(node.path);
      await refresh();
      toast.success(`已移入回收站：${result.trashedPath}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const handleDuplicateRequest = async (node: RequestNode) => {
    try {
      await requestDuplicate({ path: node.path });
      await refresh();
      toast.success("已复制请求");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  if (filtered.length === 0) {
    return (
      <>
        <ContextMenu>
          <ContextMenuTrigger className="px-3 py-6 text-center text-xs leading-relaxed text-muted-foreground">
            {needle ? "没有匹配的请求或集合" : "还没有集合。右键或用上方按钮创建第一个。"}
          </ContextMenuTrigger>
          {needle ? null : (
            <ContextMenuContent>
              <ContextMenuItem
                onClick={() => setAction({ kind: "new-collection", parentPath: null })}
              >
                新建集合
              </ContextMenuItem>
            </ContextMenuContent>
          )}
        </ContextMenu>
        <TreeDialogs action={action} onClose={() => setAction(null)} />
      </>
    );
  }

  return (
    <ContextMenu>
    <ContextMenuTrigger className="flex flex-col gap-0.5 pb-2">
      {filtered.map((node) => (
        <CollectionRow
          key={node.path}
          node={node}
          depth={0}
          activeKey={activeKey}
          collapsed={collapsed}
          onToggle={toggle}
          onOpenRequest={handleOpenRequest}
          onDeleteRequest={handleDeleteRequest}
          onDuplicateRequest={handleDuplicateRequest}
          onAction={setAction}
        />
      ))}

      <TreeDialogs action={action} onClose={() => setAction(null)} />
    </ContextMenuTrigger>
    <ContextMenuContent>
      <ContextMenuItem onClick={() => setAction({ kind: "new-collection", parentPath: null })}>
        新建集合
      </ContextMenuItem>
    </ContextMenuContent>
    </ContextMenu>
  );
}

type RowProps = {
  node: CollectionNode;
  depth: number;
  activeKey: string | null;
  collapsed: Record<string, true>;
  onToggle: (path: string) => void;
  onOpenRequest: (node: RequestNode) => void | Promise<void>;
  onDeleteRequest: (node: RequestNode) => void | Promise<void>;
  onDuplicateRequest: (node: RequestNode) => void | Promise<void>;
  onAction: (action: TreeAction) => void;
};

function CollectionRow({
  node,
  depth,
  activeKey,
  collapsed,
  onToggle,
  onOpenRequest,
  onDeleteRequest,
  onDuplicateRequest,
  onAction,
}: RowProps) {
  const isCollapsed = collapsed[node.path] === true;
  const selectedCollectionPath = useUiStore((state) => state.selectedCollectionPath);
  const selectCollection = useUiStore((state) => state.selectCollection);
  const isSelected = selectedCollectionPath === node.path;
  const refresh = useAppStore((state) => state.refresh);

  const handleDelete = async () => {
    try {
      const result = await collectionDelete(node.path);
      await refresh();
      toast.success(`已移入回收站：${result.trashedPath}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <div>
      <ContextMenu>
      <ContextMenuTrigger
        className={cn(
          "group flex items-center gap-1 rounded-lg pr-1",
          isSelected ? "bg-accent text-accent-foreground" : "hover:bg-muted",
        )}
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
      >
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-1 py-1.5 text-left"
          onClick={() => {
            onToggle(node.path);
            selectCollection(node.path);
          }}
        >
          {isCollapsed ? (
            <ChevronRightIcon className="size-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          {isCollapsed ? (
            <FolderIcon className="size-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <FolderOpenIcon className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          <span className="truncate text-sm">{node.name}</span>
        </button>

        <div className="flex shrink-0 items-center opacity-0 group-hover:opacity-100">
          <Button
            variant="ghost"
            size="icon-xs"
            title="新建请求"
            onClick={() => onAction({ kind: "new-request", collectionPath: node.path })}
          >
            <PlusIcon />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            title="重命名集合"
            onClick={() =>
              onAction({ kind: "rename-collection", path: node.path, name: node.name })
            }
          >
            <PencilIcon />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            title="删除集合（移入回收站）"
            onClick={() => void handleDelete()}
          >
            <Trash2Icon />
          </Button>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem
          onClick={() => onAction({ kind: "new-request", collectionPath: node.path })}
        >
          新建请求
        </ContextMenuItem>
        <ContextMenuItem
          onClick={() => onAction({ kind: "new-collection", parentPath: node.path })}
        >
          新建文件夹
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          onClick={() =>
            onAction({ kind: "rename-collection", path: node.path, name: node.name })
          }
        >
          重命名
        </ContextMenuItem>
        <ContextMenuItem variant="destructive" onClick={() => void handleDelete()}>
          删除
        </ContextMenuItem>
      </ContextMenuContent>
      </ContextMenu>

      {!isCollapsed ? (
        <div>
          {node.requests.map((request) => (
            <ContextMenu key={request.path}>
            <ContextMenuTrigger
              className={cn(
                "group flex items-center gap-1 rounded-lg pr-1",
                activeKey === request.path ? "bg-accent" : "hover:bg-muted",
              )}
              style={{ paddingLeft: `${(depth + 1) * 12 + 20}px` }}
            >
              <button
                type="button"
                className="flex min-w-0 flex-1 items-center gap-2 py-1.5 text-left"
                title={request.error ?? request.url ?? request.path}
                onClick={() => void onOpenRequest(request)}
              >
                <span
                  className={cn(
                    "w-11 shrink-0 text-[10px] font-semibold tracking-wide uppercase",
                    request.error ? "text-destructive" : methodColor(request.method),
                  )}
                >
                  {request.protocol === "http" ? (request.method ?? "GET") : request.protocol}
                </span>
                <span className="truncate text-sm">{request.name}</span>
              </button>

              <div className="flex shrink-0 items-center opacity-0 group-hover:opacity-100">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  title="复制请求"
                  onClick={() => void onDuplicateRequest(request)}
                >
                  <CopyIcon />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  title="重命名请求"
                  onClick={() =>
                    onAction({
                      kind: "rename-request",
                      path: request.path,
                      name: request.name,
                    })
                  }
                >
                  <PencilIcon />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  title="删除请求（移入回收站）"
                  onClick={() => void onDeleteRequest(request)}
                >
                  <Trash2Icon />
                </Button>
              </div>
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem onClick={() => void onOpenRequest(request)}>
                打开
              </ContextMenuItem>
              <ContextMenuItem onClick={() => void onDuplicateRequest(request)}>
                复制
              </ContextMenuItem>
              <ContextMenuItem
                onClick={() =>
                  onAction({
                    kind: "rename-request",
                    path: request.path,
                    name: request.name,
                  })
                }
              >
                重命名
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                onClick={() => void onDeleteRequest(request)}
              >
                删除
              </ContextMenuItem>
            </ContextMenuContent>
            </ContextMenu>
          ))}

          {node.children.map((child) => (
            <CollectionRow
              key={child.path}
              node={child}
              depth={depth + 1}
              activeKey={activeKey}
              collapsed={collapsed}
              onToggle={onToggle}
              onOpenRequest={onOpenRequest}
              onDeleteRequest={onDeleteRequest}
              onDuplicateRequest={onDuplicateRequest}
              onAction={onAction}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** 子集合操作对话框（新建请求/新建文件夹/重命名）。 */
function TreeDialogs({
  action,
  onClose,
}: {
  action: TreeAction | null;
  onClose: () => void;
}) {
  const refresh = useAppStore((state) => state.refresh);
  const [protocol, setProtocol] = useState<Protocol>("http");

  if (!action) return null;

  const config: Record<
    TreeAction["kind"],
    { title: string; label: string; confirmText: string }
  > = {
    "new-request": {
      title: "新建请求",
      label: "请求名称",
      confirmText: "创建",
    },
    "new-collection": {
      title: "新建集合 / 文件夹",
      label: "集合名称",
      confirmText: "创建",
    },
    "rename-collection": {
      title: "重命名集合",
      label: "显示名称",
      confirmText: "保存",
    },
    "rename-request": {
      title: "重命名请求",
      label: "请求名称",
      confirmText: "保存",
    },
  };

  const meta = config[action.kind];
  const defaultValue =
    action.kind === "rename-collection" || action.kind === "rename-request"
      ? action.name
      : action.kind === "new-collection"
        ? "New Collection"
        : "New Request";

  const handleConfirm = async (value: string) => {
    switch (action.kind) {
      case "new-request": {
        const detail = await requestCreate({
          collectionPath: action.collectionPath,
          name: value,
          protocol,
        });
        await refresh();
        useTabsStore.getState().openRequest({
          path: detail.path,
          name: detail.name,
          protocol: detail.protocol,
          collectionPath: detail.collectionPath,
          document: detail.raw,
        });
        useUiStore.getState().setView("request");
        break;
      }
      case "new-collection":
        await collectionCreate({
          parentPath: action.parentPath,
          name: value,
        });
        await refresh();
        break;
      case "rename-collection":
        await collectionUpdate({ path: action.path, name: value });
        await refresh();
        break;
      case "rename-request": {
        await requestUpdate({ path: action.path, name: value });
        useTabsStore.getState().renameTab(action.path, value);
        await refresh();
        break;
      }
    }
  };

  return (
    <PromptDialog
      open
      title={meta.title}
      label={meta.label}
      confirmText={meta.confirmText}
      defaultValue={defaultValue}
      onOpenChange={(open) => {
        if (!open) {
          setProtocol("http");
          onClose();
        }
      }}
      onConfirm={handleConfirm}
      extra={
        action.kind === "new-request" ? (
          <label className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-muted-foreground">协议</span>
            <SelectField
              className="w-full"
              value={protocol}
              onValueChange={(value) => setProtocol(value as Protocol)}
              options={[
                { value: "http", label: "HTTP" },
                { value: "graphql", label: "GraphQL" },
                { value: "websocket", label: "WebSocket" },
                { value: "grpc", label: "gRPC" },
              ]}
            />
          </label>
        ) : null
      }
    />
  );
}

/** 搜索过滤：保留匹配的请求与其祖先集合。 */
function filterCollections(
  nodes: CollectionNode[],
  needle: string,
): CollectionNode[] {
  const result: CollectionNode[] = [];
  for (const node of nodes) {
    const requests = node.requests.filter((request) =>
      [request.name, request.path, request.url ?? "", request.method ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(needle),
    );
    const children = filterCollections(node.children, needle);
    const selfMatches = node.name.toLowerCase().includes(needle);
    if (requests.length > 0 || children.length > 0 || selfMatches) {
      result.push({
        ...node,
        requests: selfMatches ? node.requests : requests,
        children,
      });
    }
  }
  return result;
}
