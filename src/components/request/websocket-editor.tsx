import { useState } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Input } from "@/components/ui/input";
import { normalizeKeyValues, patchDocument } from "@/lib/request-config";
import { docBool, docStringList } from "@/lib/protocol";
import type { RequestTab } from "@/stores/tabs-store";
import { useTabsStore } from "@/stores/tabs-store";
import { KeyValueEditor } from "./key-value-editor";

export function WebsocketEditor({ tab }: { tab: RequestTab }) {
  const [section, setSection] = useState("headers");
  const document = tab.document;
  const storedProtocols = docStringList(document, "protocols").join(", ");
  const [protocolsText, setProtocolsText] = useState(storedProtocols);

  const update = (patch: Parameters<typeof patchDocument>[1]) => {
    useTabsStore.getState().updateDocument(tab.key, patchDocument(document, patch));
  };

  return (
    <Tabs
      value={section}
      onValueChange={(value) => setSection(String(value))}
      className="flex min-h-0 flex-1 flex-col gap-0"
    >
      <TabsList variant="line" className="h-9 w-full shrink-0 justify-start border-b border-border px-3">
        <TabsTrigger value="headers">请求头</TabsTrigger>
        <TabsTrigger value="options">选项</TabsTrigger>
      </TabsList>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        <TabsContent value="headers" className="mt-0">
          <KeyValueEditor
            items={normalizeKeyValues(document.headers)}
            onChange={(headers) => update({ headers })}
            keyPlaceholder="Header 名称"
            valuePlaceholder="Header 值（支持 {{variable}}）"
            addLabel="添加请求头"
          />
        </TabsContent>
        <TabsContent value="options" className="mt-0 flex flex-col gap-3">
          <label className="flex flex-col gap-1.5 text-xs font-medium text-muted-foreground">
            子协议
            <Input
              className="font-mono text-xs"
              value={protocolsText}
              placeholder="graphql-transport-ws, 用逗号分隔"
              onChange={(event) => setProtocolsText(event.target.value)}
              onBlur={() =>
                update({
                  protocols: protocolsText
                    .split(",")
                    .map((item) => item.trim())
                    .filter(Boolean),
                })
              }
            />
          </label>
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              className="size-3.5 accent-primary"
              checked={docBool(document, "autoReconnect")}
              onChange={(event) => update({ autoReconnect: event.target.checked })}
            />
            断开后自动重连（最多 3 次）
          </label>
        </TabsContent>
      </div>
    </Tabs>
  );
}
