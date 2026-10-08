import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

export type SelectFieldOption = {
  value: string;
  label: string;
  disabled?: boolean;
};

type SelectFieldProps = {
  value: string;
  onValueChange: (value: string) => void;
  options: readonly SelectFieldOption[];
  placeholder?: string;
  /** 传给触发按钮的类名（宽度、颜色等）。 */
  className?: string;
  contentClassName?: string;
  size?: "sm" | "default";
  disabled?: boolean;
  id?: string;
  title?: string;
  "aria-label"?: string;
};

/**
 * 单选下拉的薄封装：`value` + `options` 的简单 API，内部使用 shadcn 的
 * base-ui `Select`（非原生弹层），保证 macOS / Windows / Linux 观感一致。
 *
 * 需要多选、搜索或分组时直接用 `@/components/ui/select` 的组合式 API。
 */
export function SelectField({
  value,
  onValueChange,
  options,
  placeholder,
  className,
  contentClassName,
  size = "default",
  disabled,
  id,
  title,
  "aria-label": ariaLabel,
}: SelectFieldProps) {
  const selected = options.find((option) => option.value === value);

  return (
    <Select
      value={value}
      onValueChange={(next) => onValueChange(String(next))}
      disabled={disabled}
    >
      <SelectTrigger
        id={id}
        size={size}
        title={title}
        aria-label={ariaLabel}
        disabled={disabled}
        className={className}
      >
        <SelectValue placeholder={placeholder}>
          {() => selected?.label ?? placeholder ?? value}
        </SelectValue>
      </SelectTrigger>
      <SelectContent className={contentClassName}>
        {options.map((option) => (
          <SelectItem
            key={option.value}
            value={option.value}
            disabled={option.disabled}
            className={cn("whitespace-nowrap")}
          >
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
