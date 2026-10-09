import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CodeEditor } from "@/components/editor/code-editor";
import { graphqlIntrospect } from "@/lib/api";
import type { RequestDetail } from "@/lib/api";
import { normalizeAuth, normalizeKeyValues, patchDocument } from "@/lib/request-config";
import {
  docString,
  errorMessage,
  graphqlOperation,
  insertGraphqlField,
  schemaRootFields,
} from "@/lib/protocol";
import { activeEnvironmentPath, useAppStore } from "@/stores/app-store";
import { useSessionStore } from "@/stores/session-store";
import type { RequestTab } from "@/stores/tabs-store";
import { useTabsStore } from "@/stores/tabs-store";
import { AuthEditor } from "./auth-editor";
import { KeyValueEditor } from "./key-value-editor";

export function GraphqlEditor({ tab, detail }: { tab: RequestTab; detail: RequestDetail | null }) {
  const [section, setSection] = useState("query");
  const [loadingSchema, setLoadingSchema] = useState(false);
  const document = tab.document;
  const query = docString(document, "query");
  const variablesText = docString(document, "variables") || "{}";
  const schema = useSessionStore((state) => state.byTab[tab.key]?.schema);
  const operation = graphqlOperation(query);
  const fields = schemaRootFields(schema, operation);

  const update = (patch: Parameters<typeof patchDocument>[1]) => {
    useTabsStore.getState().updateDocument(tab.key, patchDocument(document, patch));
  };

  const introspect = async () => {
    setLoadingSchema(true);
    try {
      const next = await graphqlIntrospect({
        endpoint: docString(document, "endpoint"),
        headers: normalizeKeyValues(document.headers),
        auth: normalizeAuth(document.auth),
        environmentPath: activeEnvironmentPath(useAppStore.getState().snapshot),
      });
      useSessionStore.getState().patch(tab.key, { schema: next });
      toast.success("已读取 GraphQL schema");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setLoadingSchema(false);
    }
  };

  return (
    <Tabs
      value={section}
      onValueChange={(value) => setSection(String(value))}
      className="flex min-h-0 flex-1 flex-col gap-0"
    >
      <TabsList variant="line" className="h-9 w-full shrink-0 justify-start border-b border-border px-3">
        <TabsTrigger value="query">查询</TabsTrigger>
        <TabsTrigger value="variables">变量</TabsTrigger>
        <TabsTrigger value="headers">请求头</TabsTrigger>
        <TabsTrigger value="auth">认证</TabsTrigger>
      </TabsList>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
        <TabsContent value="query" className="mt-0 flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              操作名
              <Input
                className="h-7 w-40 font-mono text-xs"
                value={docString(document, "operationName")}
                placeholder="可选"
                onChange={(event) => update({ operationName: event.target.value || undefined })}
              />
            </label>
            <Button
              variant="outline"
              size="sm"
              disabled={loadingSchema || !docString(document, "endpoint").trim()}
              onClick={() => void introspect()}
            >
              {loadingSchema ? "读取中" : "内省"}
            </Button>
          </div>
          {fields.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {fields.map((field) => (
                <Button
                  key={field.name}
                  variant="secondary"
                  size="xs"
                  onClick={() =>
                    update({
                      query: insertGraphqlField(query, operation, field.name, field.type),
                    })
                  }
                >
                  {field.name}
                </Button>
              ))}
            </div>
          ) : null}
          <CodeEditor
            value={query}
            language="graphql"
            minHeight={280}
            onChange={(value) => {
              if (value !== query) update({ query: value });
            }}
          />
        </TabsContent>
        <TabsContent value="variables" className="mt-0">
          <CodeEditor
            value={variablesText}
            language="json"
            minHeight={220}
            onChange={(value) => {
              if (value !== variablesText) update({ variables: value });
            }}
          />
        </TabsContent>
        <TabsContent value="headers" className="mt-0">
          <KeyValueEditor
            items={normalizeKeyValues(document.headers)}
            onChange={(headers) => update({ headers })}
            keyPlaceholder="Header 名称"
            valuePlaceholder="Header 值（支持 {{variable}}）"
            addLabel="添加请求头"
          />
        </TabsContent>
        <TabsContent value="auth" className="mt-0">
          <AuthEditor
            auth={normalizeAuth(document.auth)}
            inheritedAuth={detail?.inheritedAuth}
            onChange={(auth) => update({ auth })}
          />
        </TabsContent>
      </div>
    </Tabs>
  );
}
