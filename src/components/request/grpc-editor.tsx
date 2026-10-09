import { useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CodeEditor } from "@/components/editor/code-editor";
import { grpcLoadProto, grpcReflect, type ProtoDefinition } from "@/lib/api";
import { normalizeKeyValues, patchDocument } from "@/lib/request-config";
import { docBool, docNumber, docString, errorMessage } from "@/lib/protocol";
import { activeEnvironmentPath, useAppStore } from "@/stores/app-store";
import { useSessionStore } from "@/stores/session-store";
import type { RequestTab } from "@/stores/tabs-store";
import { useTabsStore } from "@/stores/tabs-store";
import { KeyValueEditor } from "./key-value-editor";

export function GrpcEditor({ tab }: { tab: RequestTab }) {
  const [section, setSection] = useState("message");
  const [loading, setLoading] = useState(false);
  const document = tab.document;
  const session = useSessionStore((state) => state.byTab[tab.key]);
  const services = session?.services ?? [];
  const loadedPath = useRef<string | null>(null);
  const protoSource = docString(document, "protoSource") || "file";
  const serviceName = docString(document, "service");
  const methodName = docString(document, "method");
  const service = services.find((item) => item.name === serviceName) ?? services[0];
  const methods = service?.methods ?? [];
  const payloadText = docString(document, "payload") || "{}";

  const update = (patch: Parameters<typeof patchDocument>[1]) => {
    const current =
      useTabsStore.getState().tabs.find((item) => item.key === tab.key)?.document ?? document;
    useTabsStore.getState().updateDocument(tab.key, patchDocument(current, patch));
  };

  const applyDefinition = (definition: ProtoDefinition, preferExisting: boolean) => {
    useSessionStore.getState().patch(tab.key, {
      poolId: definition.poolId,
      services: definition.services,
    });
    const current = useTabsStore.getState().tabs.find((item) => item.key === tab.key)?.document ?? document;
    const existingService = docString(current, "service");
    const existingMethod = docString(current, "method");
    const nextService =
      definition.services.find((item) => item.name === existingService) ?? definition.services[0];
    const nextMethod =
      nextService?.methods.find((item) => item.name === existingMethod) ?? nextService?.methods[0];
    if (!nextService || !nextMethod) return;
    const payload = docString(current, "payload");
    const nextPayload =
      preferExisting && payload.trim() ? payload : nextMethod.requestTemplate || "{}";
    if (
      existingService === nextService.name &&
      existingMethod === nextMethod.name &&
      payload === nextPayload
    ) {
      return;
    }
    update({
      service: nextService.name,
      method: nextMethod.name,
      payload: nextPayload,
    });
  };

  const loadFile = async (filePath: string, announce = true) => {
    setLoading(true);
    try {
      const definition = await grpcLoadProto(filePath);
      loadedPath.current = filePath;
      applyDefinition(definition, true);
      if (announce) toast.success("已加载 proto");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    const filePath = docString(document, "protoFilePath");
    if (protoSource !== "file" || !filePath || loadedPath.current === filePath) return;
    loadedPath.current = filePath;
    void loadFile(filePath, false);
    // 只在打开带路径的请求时自动加载一次。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.key]);

  const pickFile = async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "Protocol Buffers", extensions: ["proto"] }],
    });
    if (typeof picked !== "string") return;
    update({ protoSource: "file", protoFilePath: picked });
    await loadFile(picked);
  };

  const reflect = async () => {
    setLoading(true);
    try {
      const definition = await grpcReflect({
        host: docString(document, "host"),
        tlsEnabled: docBool(document, "tlsEnabled"),
        metadata: normalizeKeyValues(document.metadata),
        environmentPath: activeEnvironmentPath(useAppStore.getState().snapshot),
      });
      applyDefinition(definition, true);
      toast.success("已通过反射读取服务");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };

  const selectMethod = (nextService: string, nextMethod: string) => {
    const found = services
      .find((item) => item.name === nextService)
      ?.methods.find((item) => item.name === nextMethod);
    update({
      service: nextService,
      method: nextMethod,
      payload: found?.requestTemplate || "{}",
    });
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-3 py-2">
        <SelectField
          aria-label="描述来源"
          className="w-36"
          value={protoSource}
          onValueChange={(value) => update({ protoSource: value })}
          options={[
            { value: "file", label: "Proto 文件" },
            { value: "reflection", label: "服务反射" },
          ]}
        />
        {protoSource === "reflection" ? (
          <Button
            variant="outline"
            size="sm"
            disabled={loading || !docString(document, "host").trim()}
            onClick={() => void reflect()}
          >
            {loading ? "读取中" : "反射"}
          </Button>
        ) : (
          <Button variant="outline" size="sm" disabled={loading} onClick={() => void pickFile()}>
            {loading ? "加载中" : "选择 .proto"}
          </Button>
        )}
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {docString(document, "protoFilePath") || (session?.poolId ? "描述已加载" : "尚未加载描述")}
        </span>
        <SelectField
          aria-label="服务"
          className="w-52"
          value={service?.name ?? ""}
          placeholder="服务"
          disabled={services.length === 0}
          onValueChange={(value) => {
            const first = services.find((item) => item.name === value)?.methods[0];
            if (first) selectMethod(value, first.name);
          }}
          options={services.map((item) => ({ value: item.name, label: item.name }))}
        />
        <SelectField
          aria-label="方法"
          className="w-52"
          value={methods.some((item) => item.name === methodName) ? methodName : (methods[0]?.name ?? "")}
          placeholder="方法"
          disabled={methods.length === 0}
          onValueChange={(value) => selectMethod(service?.name ?? serviceName, value)}
          options={methods.map((item) => ({
            value: item.name,
            label: streamLabel(item.name, item.clientStreaming, item.serverStreaming),
          }))}
        />
      </div>
      <Tabs
        value={section}
        onValueChange={(value) => setSection(String(value))}
        className="flex min-h-0 flex-1 flex-col gap-0"
      >
        <TabsList variant="line" className="h-9 w-full shrink-0 justify-start border-b border-border px-3">
          <TabsTrigger value="message">消息</TabsTrigger>
          <TabsTrigger value="metadata">Metadata</TabsTrigger>
          <TabsTrigger value="options">选项</TabsTrigger>
        </TabsList>
        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          <TabsContent value="message" className="mt-0">
            <CodeEditor
              value={payloadText}
              language="json"
              minHeight={240}
              onChange={(value) => {
                if (value !== payloadText) update({ payload: value });
              }}
            />
          </TabsContent>
          <TabsContent value="metadata" className="mt-0">
            <KeyValueEditor
              items={normalizeKeyValues(document.metadata)}
              onChange={(metadata) => update({ metadata })}
              keyPlaceholder="metadata 名"
              valuePlaceholder="值（支持 {{variable}}）"
              addLabel="添加 metadata"
            />
          </TabsContent>
          <TabsContent value="options" className="mt-0">
            <label className="flex w-48 flex-col gap-1.5 text-xs font-medium text-muted-foreground">
              超时（毫秒）
              <Input
                type="number"
                min={1}
                value={docNumber(document, "timeoutMs", 30_000)}
                onChange={(event) => update({ timeoutMs: Number(event.target.value) || 30_000 })}
              />
            </label>
          </TabsContent>
        </div>
      </Tabs>
    </div>
  );
}

function streamLabel(name: string, clientStreaming: boolean, serverStreaming: boolean) {
  if (clientStreaming && serverStreaming) return `${name} · 双向`;
  if (clientStreaming) return `${name} · 客户端流`;
  if (serverStreaming) return `${name} · 服务端流`;
  return name;
}
