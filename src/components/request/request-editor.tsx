import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { RequestDetail } from "@/lib/api";
import { normalizeAuth, normalizeBody, normalizeKeyValues, normalizeSettings, patchDocument } from "@/lib/request-config";
import type { RequestTab } from "@/stores/tabs-store";
import { useTabsStore } from "@/stores/tabs-store";
import { AuthEditor } from "./auth-editor";
import { BodyEditor } from "./body-editor";
import { KeyValueEditor } from "./key-value-editor";
import { RequestSettingsPanel } from "./request-settings-panel";

type RequestEditorProps = {
  tab: RequestTab;
  detail: RequestDetail | null;
  activeTab: string;
  onActiveTabChange: (value: string) => void;
};

/** 请求编辑面板：Params / Headers / Body / Auth / Settings。 */
export function RequestEditor({
  tab,
  detail,
  activeTab,
  onActiveTabChange,
}: RequestEditorProps) {
  const document = tab.document;
  const queryParams = normalizeKeyValues(document.queryParams);
  const headers = normalizeKeyValues(document.headers);
  const body = normalizeBody(document.body);
  const auth = normalizeAuth(document.auth);
  const settings = normalizeSettings(document.settings);

  const update = (patch: Parameters<typeof patchDocument>[1]) => {
    useTabsStore
      .getState()
      .updateDocument(tab.key, patchDocument(document, patch));
  };

  const inheritCount =
    (detail?.inheritedHeaders.length ?? 0) +
    (detail?.inheritedAuth.type && detail.inheritedAuth.type !== "inherit" ? 1 : 0);

  return (
    <Tabs
      value={activeTab}
      onValueChange={(value) => onActiveTabChange(String(value))}
      className="flex min-h-0 flex-1 flex-col gap-0"
    >
      <TabsList variant="line" className="h-9 w-full shrink-0 justify-start border-b border-border px-3">
        <TabsTrigger value="params">
          参数{queryParams.length > 0 ? ` (${queryParams.length})` : ""}
        </TabsTrigger>
        <TabsTrigger value="headers">
          请求头{headers.length > 0 ? ` (${headers.length})` : ""}
        </TabsTrigger>
        <TabsTrigger value="body">
          请求体{body.type !== "none" ? " ·" : ""}
        </TabsTrigger>
        <TabsTrigger value="auth">认证</TabsTrigger>
        <TabsTrigger value="settings">
          设置{inheritCount > 0 ? " (继承)" : ""}
        </TabsTrigger>
      </TabsList>

      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        <TabsContent value="params" className="mt-0">
          <KeyValueEditor
            items={queryParams}
            onChange={(items) => update({ queryParams: items })}
            keyPlaceholder="参数名"
            valuePlaceholder="参数值（支持 {{variable}}）"
            addLabel="添加查询参数"
          />
        </TabsContent>

        <TabsContent value="headers" className="mt-0">
          <KeyValueEditor
            items={headers}
            onChange={(items) => update({ headers: items })}
            keyPlaceholder="Header 名称"
            valuePlaceholder="Header 值（支持 {{variable}}）"
            addLabel="添加请求头"
          />
        </TabsContent>

        <TabsContent value="body" className="mt-0">
          <BodyEditor body={body} onChange={(next) => update({ body: next })} />
        </TabsContent>

        <TabsContent value="auth" className="mt-0">
          <AuthEditor
            auth={auth}
            onChange={(next) => update({ auth: next })}
            inheritedAuth={detail?.inheritedAuth ?? null}
          />
        </TabsContent>

        <TabsContent value="settings" className="mt-0">
          <RequestSettingsPanel
            settings={settings}
            onChange={(next) => update({ settings: next })}
            inheritedHeaders={detail?.inheritedHeaders ?? []}
            inheritedAuth={detail?.inheritedAuth ?? { type: "inherit" }}
            requestPath={tab.path}
          />
        </TabsContent>
      </div>
    </Tabs>
  );
}
