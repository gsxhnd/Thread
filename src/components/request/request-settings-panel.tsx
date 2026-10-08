import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import type { AuthSpec, KeyValue, RequestSettings } from "@/lib/api";

type RequestSettingsPanelProps = {
  settings: RequestSettings;
  onChange: (settings: RequestSettings) => void;
  inheritedHeaders: KeyValue[];
  inheritedAuth: AuthSpec;
  requestPath?: string | null;
};

/** 请求级设置 + 集合继承信息（超时 / 重定向 / 证书校验）。 */
export function RequestSettingsPanel({
  settings,
  onChange,
  inheritedHeaders,
  inheritedAuth,
  requestPath,
}: RequestSettingsPanelProps) {
  return (
    <div className="space-y-5">
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="block space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground">
            超时（毫秒，留空继承应用默认值）
          </span>
          <Input
            type="number"
            min={1}
            value={settings.timeoutMs ?? ""}
            placeholder="继承"
            onChange={(event) =>
              onChange({
                ...settings,
                timeoutMs: event.target.value ? Number(event.target.value) : undefined,
              })
            }
          />
        </label>

        <label className="block space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground">跟随重定向</span>
          <SelectField
            value={triStateValue(settings.followRedirects)}
            onValueChange={(value) =>
              onChange({ ...settings, followRedirects: triStateParse(value) })
            }
            options={[
              { value: "inherit", label: "继承" },
              { value: "true", label: "启用" },
              { value: "false", label: "禁用" },
            ]}
          />
        </label>

        <label className="block space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground">
            校验证书（HTTPS）
          </span>
          <SelectField
            value={triStateValue(settings.verifySsl)}
            onValueChange={(value) =>
              onChange({ ...settings, verifySsl: triStateParse(value) })
            }
            options={[
              { value: "inherit", label: "继承" },
              { value: "true", label: "启用" },
              { value: "false", label: "禁用（自签名证书调试）" },
            ]}
          />
        </label>
      </div>

      <section className="space-y-2 rounded-lg border border-border p-3">
        <h3 className="text-xs font-medium text-muted-foreground">继承自集合</h3>
        {inheritedHeaders.length === 0 && inheritedAuth.type === "inherit" ? (
          <p className="text-xs text-muted-foreground">集合未配置共享 Headers / 认证</p>
        ) : (
          <ul className="space-y-1 text-xs">
            {inheritedHeaders.map((header) => (
              <li key={header.key} className="font-mono">
                {header.key}: {header.value}
              </li>
            ))}
            {inheritedAuth.type && inheritedAuth.type !== "inherit" ? (
              <li>认证：{inheritedAuth.type}</li>
            ) : null}
          </ul>
        )}
      </section>

      <section className="space-y-1 text-xs text-muted-foreground">
        <p>
          工作区文件：
          <span className="ml-1 font-mono text-foreground">
            {requestPath ?? "未保存的草稿（保存后写入集合目录）"}
          </span>
        </p>
        <p>变量替换（{"{{name}}"}）在 Rust 侧发送前完成，机密变量值保存在本地数据目录。</p>
      </section>
    </div>
  );
}

function triStateValue(value: boolean | null | undefined): string {
  if (value === undefined || value === null) return "inherit";
  return value ? "true" : "false";
}

function triStateParse(value: string): boolean | undefined {
  if (value === "true") return true;
  if (value === "false") return false;
  return undefined;
}
