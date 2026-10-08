import { WandSparklesIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import { CodeEditor } from "@/components/editor/code-editor";
import { prettyPrintJson } from "@/lib/format";
import { detectLanguage } from "@/lib/editor/monaco";
import type { HttpBody, HttpBodyField, HttpBodyType } from "@/lib/api";
import { KeyValueEditor } from "./key-value-editor";
import { VariableInput } from "./variable-input";

const BODY_TYPES: { value: HttpBodyType; label: string }[] = [
  { value: "none", label: "无" },
  { value: "json", label: "JSON" },
  { value: "raw", label: "Raw" },
  { value: "formUrlencoded", label: "Form (x-www-form-urlencoded)" },
  { value: "multipart", label: "Form Data (multipart)" },
  { value: "binary", label: "Binary（文件）" },
];

type BodyEditorProps = {
  body: HttpBody;
  onChange: (body: HttpBody) => void;
};

/** 请求体编辑器：JSON / Raw / 表单 / 文件上传 / 二进制文件。 */
export function BodyEditor({ body, onChange }: BodyEditorProps) {
  const fields = body.fields ?? [];

  const setFields = (next: HttpBodyField[]) => onChange({ ...body, fields: next });

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <SelectField
          className="w-64"
          value={body.type}
          onValueChange={(type) => onChange({ ...body, type: type as HttpBodyType })}
          options={BODY_TYPES}
        />

        {body.type === "json" ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() => onChange({ ...body, content: prettyPrintJson(body.content) })}
          >
            <WandSparklesIcon /> 格式化
          </Button>
        ) : null}

        {body.type === "raw" ? (
          <Input
            className="w-64 font-mono text-xs"
            value={body.contentType ?? ""}
            placeholder="Content-Type（默认 text/plain）"
            onChange={(event) =>
              onChange({ ...body, contentType: event.target.value })
            }
          />
        ) : null}
      </div>

      {body.type === "none" ? (
        <p className="text-xs text-muted-foreground">该请求不发送请求体</p>
      ) : null}

      {body.type === "json" || body.type === "raw" ? (
        <CodeEditor
          value={body.content}
          language={body.type === "json" ? "json" : detectLanguage(body.content)}
          onChange={(value) => onChange({ ...body, content: value })}
          minHeight={260}
        />
      ) : null}

      {body.type === "formUrlencoded" ? (
        <KeyValueEditor
          items={fields.map((field) => ({
            key: field.key,
            value: field.value,
            enabled: field.enabled,
          }))}
          onChange={(items) =>
            setFields(
              items.map((item) => ({
                key: item.key,
                value: item.value,
                enabled: item.enabled,
                kind: "text" as const,
              })),
            )
          }
          keyPlaceholder="字段名"
          valuePlaceholder="字段值"
          addLabel="添加字段"
        />
      ) : null}

      {body.type === "multipart" ? (
        <div className="space-y-1.5">
          {fields.length === 0 ? (
            <p className="px-1 py-2 text-xs text-muted-foreground">暂无字段</p>
          ) : null}
          {fields.map((field, index) => (
            <div key={index} className="flex items-center gap-1.5">
              <input
                type="checkbox"
                className="size-3.5 shrink-0 accent-primary"
                checked={field.enabled}
                onChange={(event) =>
                  setFields(
                    fields.map((item, position) =>
                      position === index
                        ? { ...item, enabled: event.target.checked }
                        : item,
                    ),
                  )
                }
              />
              <VariableInput
                className="w-40"
                inputClassName="font-mono text-xs"
                value={field.key}
                placeholder="字段名"
                onValueChange={(key) =>
                  setFields(
                    fields.map((item, position) =>
                      position === index ? { ...item, key } : item,
                    ),
                  )
                }
              />
              <SelectField
                className="w-24"
                value={field.kind}
                onValueChange={(kind) =>
                  setFields(
                    fields.map((item, position) =>
                      position === index
                        ? { ...item, kind: kind as "text" | "file" }
                        : item,
                    ),
                  )
                }
                options={[
                  { value: "text", label: "文本" },
                  { value: "file", label: "文件" },
                ]}
              />
              <VariableInput
                className="flex-1"
                inputClassName="font-mono text-xs"
                value={field.kind === "file" ? (field.filePath ?? "") : field.value}
                placeholder={field.kind === "file" ? "/绝对路径/文件.txt" : "字段值"}
                onValueChange={(next) =>
                  setFields(
                    fields.map((item, position) =>
                      position === index
                        ? field.kind === "file"
                          ? { ...item, filePath: next }
                          : { ...item, value: next }
                        : item,
                    ),
                  )
                }
              />
              <Button
                variant="ghost"
                size="icon-xs"
                title="删除字段"
                onClick={() =>
                  setFields(fields.filter((_, position) => position !== index))
                }
              >
                删除
              </Button>
            </div>
          ))}
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              setFields([
                ...fields,
                { key: "", value: "", kind: "text", enabled: true },
              ])
            }
          >
            添加字段
          </Button>
        </div>
      ) : null}

      {body.type === "binary" ? (
        <label className="block space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground">
            文件绝对路径（作为请求体原样发送）
          </span>
          <VariableInput
            inputClassName="font-mono text-xs"
            value={body.filePath ?? ""}
            placeholder="/Users/you/data.bin 或 {{fixtures}}/data.bin"
            onValueChange={(filePath) => onChange({ ...body, filePath })}
          />
        </label>
      ) : null}
    </div>
  );
}
