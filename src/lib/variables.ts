/**
 * `{{variable}}` 补全与替换的纯函数工具。
 *
 * 变量名来自工作区索引（Rust 侧只发送变量名，机密值不外发），
 * 作用域顺序与后端替换一致：激活环境 → 全局环境 → 其他环境；
 * 另外提供与 Rust 一致的动态内置变量。
 */
import type { WorkspaceSnapshot } from "@/lib/api";

export interface VariableSuggestion {
  key: string;
  /** 变量来源（环境名或「内置」）。 */
  source: string;
  /** 值为本地存储（界面掩码显示）。 */
  secret: boolean;
  /** 是否来自当前激活环境。 */
  active: boolean;
  builtin?: boolean;
}

/** 与 `src-tauri/src/services/variables.rs::resolve_builtin` 保持一致。 */
export const BUILTIN_VARIABLES = [
  { key: "$uuid", description: "随机 UUID v4" },
  { key: "$timestamp", description: "Unix 时间戳（秒）" },
  { key: "$isoTimestamp", description: "ISO 8601 时间" },
  { key: "$randomInt", description: "随机整数" },
] as const;

/**
 * 汇总可补全的变量：激活环境优先，其次是全局环境，最后是其余环境；同名去重。
 */
export function collectVariableSuggestions(
  snapshot: WorkspaceSnapshot | null,
): VariableSuggestion[] {
  const environments = snapshot?.index.environments ?? [];
  const active = environments.filter((environment) => environment.isActive);
  const global = environments.filter(
    (environment) => environment.isGlobal && !environment.isActive,
  );
  const others = environments.filter(
    (environment) => !environment.isActive && !environment.isGlobal,
  );

  const seen = new Set<string>();
  const suggestions: VariableSuggestion[] = [];

  for (const environment of [...active, ...global, ...others]) {
    const secretNames = new Set(environment.secretVariableNames ?? []);
    for (const key of environment.variableNames ?? []) {
      if (!key || seen.has(key)) continue;
      seen.add(key);
      suggestions.push({
        key,
        source: environment.name,
        secret: secretNames.has(key),
        active: environment.isActive,
      });
    }
  }

  for (const builtin of BUILTIN_VARIABLES) {
    if (seen.has(builtin.key)) continue;
    seen.add(builtin.key);
    suggestions.push({
      key: builtin.key,
      source: "内置",
      secret: false,
      active: true,
      builtin: true,
    });
  }

  return suggestions;
}

/** 按输入前缀过滤（支持 `$` 内置变量与普通变量名）。 */
export function filterSuggestions(
  suggestions: VariableSuggestion[],
  query: string,
  limit = 8,
): VariableSuggestion[] {
  const needle = query.toLowerCase();
  const matched = needle
    ? suggestions.filter((item) => item.key.toLowerCase().includes(needle))
    : suggestions;
  return [...matched]
    .sort((a, b) => {
      const aStarts = a.key.toLowerCase().startsWith(needle) ? 0 : 1;
      const bStarts = b.key.toLowerCase().startsWith(needle) ? 0 : 1;
      if (aStarts !== bStarts) return aStarts - bStarts;
      if (a.active !== b.active) return a.active ? -1 : 1;
      return a.key.localeCompare(b.key);
    })
    .slice(0, limit);
}

export interface VariableQuery {
  /** `{{` 起始下标。 */
  start: number;
  /** 已输入的前缀（可能是空串，表示刚敲完 `{{`）。 */
  query: string;
}

/**
 * 判断光标是否处于一个尚未闭合的 `{{ ...` 中。
 *
 * 返回 `{{` 的位置与已输入前缀，供补全列表替换使用。
 */
export function findVariableQuery(
  text: string,
  caret: number,
): VariableQuery | null {
  const before = text.slice(0, Math.max(0, Math.min(caret, text.length)));
  const open = before.lastIndexOf("{{");
  if (open === -1) return null;
  const tail = before.slice(open + 2);
  // 已闭合（`}}`）、跨行、嵌套花括号、或名字里出现空白 → 不是在输入变量名。
  if (/[\s{}]/.test(tail)) return null;
  return { start: open, query: tail };
}

/** 把 `{{prefix` 替换为 `{{key}}`，并把光标放到变量之后。 */
export function applyVariableCompletion(
  text: string,
  caret: number,
  active: VariableQuery,
  key: string,
): { value: string; caret: number } {
  const head = text.slice(0, active.start);
  const afterCaret = text.slice(caret);
  // 光标后若已经有 `}}`（例如补全发生在已有占位符内部），一并吃掉，避免 `}}}}`。
  const tail = afterCaret.startsWith("}}") ? afterCaret.slice(2) : afterCaret;
  const inserted = `{{${key}}}`;
  return {
    value: `${head}${inserted}${tail}`,
    caret: active.start + inserted.length,
  };
}

/**
 * 在任意光标位置插入完整占位符 `{{key}}`（用于「变量」按钮手动插入）。
 *
 * 与 `applyVariableCompletion` 的差别：这里不依赖已存在的 `{{` 上下文，
 * 会替换当前选区，若光标后紧跟 `}}` 也会一并吃掉，避免出现 `{{key}}}}`。
 */
export function insertVariablePlaceholder(
  text: string,
  selectionStart: number,
  selectionEnd: number,
  key: string,
): { value: string; caret: number } {
  const start = Math.max(0, Math.min(selectionStart, text.length));
  const end = Math.max(start, Math.min(selectionEnd, text.length));
  let head = text.slice(0, start);
  let tail = text.slice(end);
  // 光标正好落在一个空的 `{{}}` 中间时，整体替换而不是插入嵌套占位符。
  if (head.endsWith("{{") && tail.startsWith("}}")) {
    head = head.slice(0, -2);
    tail = tail.slice(2);
  }
  const inserted = `{{${key}}}`;
  return {
    value: `${head}${inserted}${tail}`,
    caret: head.length + inserted.length,
  };
}

/** 取反：生成变量占位文本（用于提示缺失变量）。 */
export function variablePlaceholder(key: string): string {
  return `{{${key}}}`;
}
