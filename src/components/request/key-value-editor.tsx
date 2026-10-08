import { PlusIcon, Trash2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { KeyValue } from "@/lib/api";
import { VariableInput } from "./variable-input";

type KeyValueEditorProps = {
  items: KeyValue[];
  onChange: (items: KeyValue[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
  /** 值以掩码显示（机密变量）。 */
  maskValues?: boolean;
};

/** Query Params / Headers / 表单字段的通用键值编辑器。 */
export function KeyValueEditor({
  items,
  onChange,
  keyPlaceholder = "key",
  valuePlaceholder = "value",
  addLabel = "添加一行",
  maskValues = false,
}: KeyValueEditorProps) {
  const update = (index: number, patch: Partial<KeyValue>) => {
    onChange(items.map((item, position) => (position === index ? { ...item, ...patch } : item)));
  };

  const remove = (index: number) => {
    onChange(items.filter((_, position) => position !== index));
  };

  const add = () => {
    onChange([...items, { key: "", value: "", enabled: true }]);
  };

  return (
    <div className="flex flex-col gap-1.5">
      {items.length === 0 ? (
        <p className="px-1 py-2 text-xs text-muted-foreground">暂无条目</p>
      ) : null}

      {items.map((item, index) => (
        <div
          key={index}
          className={cn("flex items-center gap-1.5", item.enabled ? undefined : "opacity-50")}
        >
          <input
            type="checkbox"
            className="size-3.5 shrink-0 accent-primary"
            checked={item.enabled}
            title={item.enabled ? "已启用" : "已禁用"}
            onChange={(event) => update(index, { enabled: event.target.checked })}
          />
          <VariableInput
            className="w-1/3 min-w-32"
            inputClassName="font-mono text-xs"
            value={item.key}
            placeholder={keyPlaceholder}
            onValueChange={(key) => update(index, { key })}
          />
          <VariableInput
            className="flex-1"
            inputClassName="font-mono text-xs"
            type={maskValues ? "password" : "text"}
            value={item.value}
            placeholder={valuePlaceholder}
            onValueChange={(value) => update(index, { value })}
          />
          <Button
            variant="ghost"
            size="icon-xs"
            title="删除该行"
            onClick={() => remove(index)}
          >
            <Trash2Icon />
          </Button>
        </div>
      ))}

      <Button variant="outline" size="sm" onClick={add}>
        <PlusIcon /> {addLabel}
      </Button>
    </div>
  );
}
