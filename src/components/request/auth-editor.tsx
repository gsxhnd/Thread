import { SelectField } from "@/components/ui/select-field";
import type { AuthSpec } from "@/lib/api";
import { VariableInput } from "./variable-input";

const AUTH_TYPES: { value: string; label: string }[] = [
  { value: "inherit", label: "继承集合配置" },
  { value: "none", label: "无认证" },
  { value: "bearer", label: "Bearer Token" },
  { value: "basic", label: "Basic Auth" },
  { value: "apiKey", label: "API Key" },
];

type AuthEditorProps = {
  auth: AuthSpec;
  onChange: (auth: AuthSpec) => void;
  /** 集合级认证（用于展示继承结果）。 */
  inheritedAuth?: AuthSpec | null;
};

/** 认证编辑器；`inherit` 表示沿用集合级配置。 */
export function AuthEditor({ auth, onChange, inheritedAuth }: AuthEditorProps) {
  const inheritedLabel =
    inheritedAuth && inheritedAuth.type && inheritedAuth.type !== "inherit"
      ? `当前继承：${inheritedAuth.type}`
      : "集合未配置认证";

  return (
    <div className="space-y-3">
      <label className="block space-y-1.5">
        <span className="text-xs font-medium text-muted-foreground">认证方式</span>
        <SelectField
          className="w-64"
          value={auth.type ?? "inherit"}
          onValueChange={(type) => onChange({ ...auth, type })}
          options={AUTH_TYPES}
        />
      </label>

      {auth.type === "inherit" ? (
        <p className="text-xs text-muted-foreground">{inheritedLabel}</p>
      ) : null}

      {auth.type === "bearer" ? (
        <label className="block space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground">
            Token（支持 {"{{variable}}"}）
          </span>
          <VariableInput
            inputClassName="font-mono text-xs"
            value={auth.token ?? ""}
            placeholder="{{apiKey}}"
            onValueChange={(token) => onChange({ ...auth, token })}
          />
        </label>
      ) : null}

      {auth.type === "basic" ? (
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">用户名</span>
            <VariableInput
              value={auth.username ?? ""}
              placeholder="{{username}}"
              onValueChange={(username) => onChange({ ...auth, username })}
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">密码</span>
            <VariableInput
              type="password"
              value={auth.password ?? ""}
              placeholder="{{password}}"
              onValueChange={(password) => onChange({ ...auth, password })}
            />
          </label>
        </div>
      ) : null}

      {auth.type === "apiKey" ? (
        <div className="grid gap-3 sm:grid-cols-[1fr_1fr_auto]">
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">参数名</span>
            <VariableInput
              inputClassName="font-mono text-xs"
              value={auth.key ?? ""}
              placeholder="X-Api-Key"
              onValueChange={(key) => onChange({ ...auth, key })}
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">值</span>
            <VariableInput
              inputClassName="font-mono text-xs"
              value={auth.value ?? ""}
              placeholder="{{apiKey}}"
              onValueChange={(value) => onChange({ ...auth, value })}
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">位置</span>
            <SelectField
              value={auth.in ?? "header"}
              onValueChange={(value) =>
                onChange({ ...auth, in: value === "query" ? "query" : "header" })
              }
              options={[
                { value: "header", label: "Header" },
                { value: "query", label: "Query" },
              ]}
            />
          </label>
        </div>
      ) : null}
    </div>
  );
}
