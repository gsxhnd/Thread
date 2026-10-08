import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { CheckIcon, LockIcon, SparklesIcon, VariableIcon } from "lucide-react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app-store";
import {
  applyVariableCompletion,
  collectVariableSuggestions,
  filterSuggestions,
  findVariableQuery,
  insertVariablePlaceholder,
  type VariableQuery,
  type VariableSuggestion,
} from "@/lib/variables";

/**
 * 候选行里的 `{{key}}`：把已输入的前缀在 key 内高亮（不区分大小写），
 * 便于在一屏候选中确认匹配到了哪几个字符。
 */
function VariableKeyLabel({ name, query }: { name: string; query: string }) {
  const at = query ? name.toLowerCase().indexOf(query.toLowerCase()) : -1;
  if (at < 0) return <>{`{{${name}}}`}</>;
  return (
    <>
      {"{{"}
      {name.slice(0, at)}
      <span className="rounded-[3px] bg-primary/20 font-semibold text-primary">
        {name.slice(at, at + query.length)}
      </span>
      {name.slice(at + query.length)}
      {"}}"}
    </>
  );
}

/** 输入框内一个已闭合占位符 `{{key}}` 的位置（相对输入框边框盒，已计入横向滚动）。 */
type PlaceholderRect = {
  left: number;
  top: number;
  width: number;
  height: number;
};

/** 只给已闭合的 `{{...}}` 上色；正在输入的 `{{abc` 不上色，避免光标处闪色块。 */
const PLACEHOLDER_PATTERN = /\{\{[^{}\n]+\}\}/g;

let measureCanvas: HTMLCanvasElement | null = null;

/** 用 canvas 量文本宽度：需要与输入框完全相同的字体，否则高亮块会错位。 */
function measureTextWidth(text: string, font: string): number {
  if (!measureCanvas) measureCanvas = document.createElement("canvas");
  const context = measureCanvas.getContext("2d");
  if (!context) return 0;
  context.font = font;
  return context.measureText(text).width;
}

/**
 * 量出输入框里所有 `{{变量}}` 的位置。
 *
 * `<input>` 无法对局部文本着色，因此这里按输入框的**计算样式**（字体、内边距、边框、
 * 滚动量）算出一组矩形，交给上层画在文字下方的高亮块——文字本身保持原样，
 * 不需要透明文字，也不影响光标、选区与输入法。
 */
function measurePlaceholders(input: HTMLInputElement): PlaceholderRect[] {
  const text = input.value;
  if (!text.includes("{{")) return [];

  const style = getComputedStyle(input);
  const font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  const originLeft = parseFloat(style.paddingLeft) + parseFloat(style.borderLeftWidth);
  const originTop = parseFloat(style.paddingTop) + parseFloat(style.borderTopWidth);
  const height = Math.max(
    0,
    input.clientHeight -
      originTop -
      parseFloat(style.paddingBottom) -
      parseFloat(style.borderBottomWidth),
  );

  const rects: PlaceholderRect[] = [];
  for (const match of text.matchAll(PLACEHOLDER_PATTERN)) {
    const start = match.index ?? 0;
    rects.push({
      left: originLeft + measureTextWidth(text.slice(0, start), font) - input.scrollLeft,
      top: originTop,
      width: measureTextWidth(match[0], font),
      height,
    });
  }
  return rects;
}

function samePlaceholders(a: PlaceholderRect[], b: PlaceholderRect[]): boolean {
  return (
    a.length === b.length &&
    a.every((rect, index) => {
      const other = b[index];
      if (!other) return false;
      return (
        rect.left === other.left &&
        rect.top === other.top &&
        rect.width === other.width &&
        rect.height === other.height
      );
    })
  );
}

type VariableInputProps = {
  value: string;
  onValueChange: (value: string) => void;
  placeholder?: string;
  className?: string;
  inputClassName?: string;
  type?: string;
  autoFocus?: boolean;
  disabled?: boolean;
  /** 在输入框右侧常驻一个「插入变量」按钮（URL 框使用，便于发现可用变量）。 */
  withPicker?: boolean;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
  onBlur?: (event: FocusEvent<HTMLInputElement>) => void;
};

/**
 * 支持 `{{变量}}` 自动补全的单行输入框。
 *
 * 输入 `{{` 后弹出当前工作区的变量列表（激活环境优先，含内置变量），
 * ↑/↓ 选择（带高亮）、Enter/Tab 或点击插入完整的 `{{key}}`；机密变量显示锁标记。
 * 传入 `withPicker` 时右侧还有一个按钮，可在任意光标位置插入变量。
 * 未处于 `{{` 上下文且列表未打开时不拦截任何按键（例如 URL 框的 Enter 发送）。
 */
export function VariableInput({
  value,
  onValueChange,
  placeholder,
  className,
  inputClassName,
  type = "text",
  autoFocus,
  disabled,
  withPicker,
  onKeyDown,
  onBlur,
}: VariableInputProps) {
  const snapshot = useAppStore((state) => state.snapshot);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const [context, setContext] = useState<VariableQuery | null>(null);
  /**
   * 由右侧按钮打开的列表：`{{` 上下文不属于输入内容，
   * 因此不受光标移动影响，只在输入、插入、Esc 或失焦时关闭。
   */
  const [manual, setManual] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  /** 上一次的补全上下文标识，用于判断是否需要把高亮重置回第一项。 */
  const contextKeyRef = useRef<string | null>(null);
  const listboxId = useRef(`variable-suggestions-${Math.random().toString(36).slice(2, 9)}`);

  const suggestions = useMemo(() => collectVariableSuggestions(snapshot), [snapshot]);
  const matches = context ? filterSuggestions(suggestions, context.query) : [];
  const open = context !== null && matches.length > 0;
  const activeIndexSafe = matches.length > 0 ? Math.min(activeIndex, matches.length - 1) : 0;

  const [placeholders, setPlaceholders] = useState<PlaceholderRect[]>([]);

  /** 重新测量 `{{变量}}` 位置；结果不变时保持原引用，避免多余重渲染。 */
  const measure = useCallback(() => {
    const input = inputRef.current;
    if (!input) return;
    const next = measurePlaceholders(input);
    setPlaceholders((prev) => (samePlaceholders(prev, next) ? prev : next));
  }, []);

  // 输入内容变化后（含程序化插入）同步高亮块位置。
  useLayoutEffect(() => {
    measure();
  }, [measure, value]);

  // 字体异步加载完成、或容器尺寸变化会让文本宽度改变，需要重新测量。
  useEffect(() => {
    const onResize = () => measure();
    window.addEventListener("resize", onResize);
    void document.fonts?.ready.then(onResize, () => {});
    return () => window.removeEventListener("resize", onResize);
  }, [measure]);

  // 键盘选择时把高亮行滚入可视区域（列表最多 8 行，通常无需滚动，但窄窗口下有用）。
  useEffect(() => {
    if (!open) return;
    const row = listRef.current?.querySelector<HTMLElement>(`[data-index="${activeIndexSafe}"]`);
    row?.scrollIntoView({ block: "nearest" });
  }, [open, activeIndexSafe]);

  /**
   * 更新补全上下文。
   *
   * `resetIndex` 只在「输入内容变化」时为 true；光标移动（方向键/点击）不重置高亮，
   * 否则 ↑/↓ 会立刻被随后的 keyup 撤销。
   */
  const applyContext = (next: VariableQuery | null, resetIndex: boolean) => {
    const key = next ? `${next.start}:${next.query}` : null;
    const changed = key !== contextKeyRef.current;
    contextKeyRef.current = key;
    setContext(next);
    if (resetIndex || changed) setActiveIndex(0);
  };

  /** 输入内容变化：重新匹配并在查询串变化时把高亮移回第一项。 */
  const syncFromText = (text: string, caret: number | null) => {
    setManual(false);
    applyContext(caret === null ? null : findVariableQuery(text, caret), true);
  };

  /** 光标移动（keyup / click）：上下文不变时保留高亮。 */
  const syncFromCaret = (text: string, caret: number | null) => {
    if (manual) return;
    applyContext(caret === null ? null : findVariableQuery(text, caret), false);
  };

  const close = () => {
    contextKeyRef.current = null;
    setContext(null);
    setManual(false);
  };

  const accept = (suggestion: VariableSuggestion) => {
    const input = inputRef.current;
    if (!input) return;
    const caret = input.selectionStart ?? input.value.length;
    const active = findVariableQuery(input.value, caret);
    const { value: nextValue, caret: nextCaret } = active
      ? applyVariableCompletion(input.value, caret, active, suggestion.key)
      : insertVariablePlaceholder(
          input.value,
          input.selectionStart ?? caret,
          input.selectionEnd ?? caret,
          suggestion.key,
        );
    onValueChange(nextValue);
    close();
    requestAnimationFrame(() => {
      input.focus();
      input.setSelectionRange(nextCaret, nextCaret);
    });
  };

  const openPicker = (event: MouseEvent<HTMLButtonElement>) => {
    // 保持输入框焦点，避免 blur 先关闭列表。
    event.preventDefault();
    const input = inputRef.current;
    if (!input || disabled) return;
    if (document.activeElement !== input) {
      input.focus();
      const end = input.value.length;
      input.setSelectionRange(end, end);
    }
    const caret = input.selectionStart ?? input.value.length;
    const active = findVariableQuery(input.value, caret);
    if (active) {
      setManual(false);
      contextKeyRef.current = `${active.start}:${active.query}`;
      setContext(active);
    } else {
      setManual(true);
      contextKeyRef.current = null;
      setContext({ start: caret, query: "" });
    }
    setActiveIndex(0);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (open) {
      const last = matches.length - 1;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((index) => (Math.min(index, last) + 1) % matches.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((index) => (Math.min(index, last) - 1 + matches.length) % matches.length);
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        const suggestion = matches[Math.min(activeIndex, last)];
        if (suggestion) accept(suggestion);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        close();
        return;
      }
    }
    onKeyDown?.(event);
  };

  const handleRowMouseDown = (
    event: MouseEvent<HTMLButtonElement>,
    suggestion: VariableSuggestion,
  ) => {
    // 保持输入框焦点，避免 blur 先关闭列表。
    event.preventDefault();
    accept(suggestion);
  };

  return (
    <div className={cn("relative min-w-0", className)}>
      {/*
        `{{变量}}` 高亮层：位于文字之下（输入框 z-10 覆盖其上），
        只画面色块，文字/光标/选区/输入法全部交给原生 input。
      */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 z-0 overflow-hidden"
      >
        {placeholders.map((rect, index) => (
          <span
            key={`${rect.left}:${rect.width}:${index}`}
            className="absolute rounded-[4px] bg-primary/15 ring-1 ring-primary/30 ring-inset"
            style={{
              left: rect.left,
              top: rect.top,
              width: rect.width,
              height: rect.height,
            }}
          />
        ))}
      </div>

      <Input
        ref={inputRef}
        type={type}
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        autoFocus={autoFocus}
        className={cn("relative z-10", withPicker && "pr-7", inputClassName)}
        role="combobox"
        aria-expanded={open}
        aria-controls={open ? listboxId.current : undefined}
        aria-autocomplete="list"
        aria-activedescendant={
          open ? `${listboxId.current}-option-${activeIndexSafe}` : undefined
        }
        onChange={(event) => {
          onValueChange(event.target.value);
          syncFromText(event.target.value, event.target.selectionStart);
        }}
        onKeyDown={handleKeyDown}
        onKeyUp={(event) => {
          const target = event.target as HTMLInputElement;
          syncFromCaret(target.value, target.selectionStart);
        }}
        onClick={(event) => {
          const target = event.target as HTMLInputElement;
          syncFromCaret(target.value, target.selectionStart);
        }}
        onScroll={measure}
        onBlur={(event) => {
          close();
          onBlur?.(event);
        }}
      />

      {withPicker ? (
        <button
          type="button"
          aria-label="插入变量"
          title="插入变量 {{...}}"
          onMouseDown={openPicker}
          className="absolute top-1/2 right-1 z-20 -translate-y-1/2 rounded-md p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          <VariableIcon className="size-3.5" />
        </button>
      ) : null}

      {open ? (
        <div
          ref={listRef}
          id={listboxId.current}
          role="listbox"
          data-slot="variable-suggestions"
          className="absolute top-full left-0 z-50 mt-1 max-h-64 w-80 overflow-y-auto rounded-lg border border-border bg-popover p-1 shadow-lg"
        >
          <p className="flex items-center gap-1.5 px-2 py-1 text-[10px] text-muted-foreground">
            <VariableIcon className="size-3" />
            变量（↑↓ 选择，Enter/Tab 插入）
          </p>
          {matches.map((suggestion, index) => (
            <button
              key={suggestion.key}
              id={`${listboxId.current}-option-${index}`}
              data-index={index}
              role="option"
              aria-selected={index === activeIndexSafe}
              type="button"
              tabIndex={-1}
              onMouseDown={(event) => handleRowMouseDown(event, suggestion)}
              onMouseEnter={() => setActiveIndex(index)}
              className={cn(
                "flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs",
                index === activeIndexSafe
                  ? "bg-accent font-medium text-accent-foreground ring-1 ring-ring/40 ring-inset"
                  : "text-foreground/90 hover:bg-muted/60",
              )}
            >
              {index === activeIndexSafe ? (
                <CheckIcon className="size-3 shrink-0 text-primary" />
              ) : (
                <span className="size-3 shrink-0" />
              )}
              <span className="min-w-0 flex-1 truncate font-mono">
                <VariableKeyLabel name={suggestion.key} query={context?.query ?? ""} />
              </span>
              {suggestion.builtin ? (
                <SparklesIcon className="size-3 shrink-0 text-notice" />
              ) : null}
              {suggestion.secret ? (
                <LockIcon className="size-3 shrink-0 text-muted-foreground" />
              ) : null}
              <span className="shrink-0 text-[10px] text-muted-foreground">
                {suggestion.source}
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
