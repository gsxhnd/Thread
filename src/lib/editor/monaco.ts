/**
 * Monaco Editor 本地化装配。
 *
 * 使用 npm 包内的 ESM 入口（不走 CDN），满足桌面离线运行与 CSP
 * （`worker-src 'self' blob:`）。编辑器用于请求 Body 编辑与响应只读展示。
 */
import * as monaco from "monaco-editor";
import { findVariableQuery } from "@/lib/variables";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import "monaco-editor/min/vs/editor/editor.main.css";

type MonacoEnvironmentLike = {
  getWorker: (workerId: string, label: string) => Worker;
};

const environment: MonacoEnvironmentLike = {
  getWorker(_workerId, label) {
    if (label === "json") {
      return new jsonWorker();
    }
    return new editorWorker();
  },
};

(self as unknown as { MonacoEnvironment: MonacoEnvironmentLike }).MonacoEnvironment =
  environment;

export { monaco };

export const MONACO_THEME_LIGHT = "thread-light";
export const MONACO_THEME_DARK = "thread-dark";

let themesDefined = false;

/** 定义与应用主题一致的编辑器主题（只注册一次）。 */
export function ensureMonacoThemes() {
  if (themesDefined) return;
  themesDefined = true;
  monaco.editor.defineTheme(MONACO_THEME_LIGHT, {
    base: "vs",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": "#00000000",
      "editorGutter.background": "#00000000",
    },
  });
  monaco.editor.defineTheme(MONACO_THEME_DARK, {
    base: "vs-dark",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": "#00000000",
      "editorGutter.background": "#00000000",
    },
  });
}

export function currentMonacoTheme(): string {
  const isDark =
    typeof document !== "undefined" &&
    document.documentElement.classList.contains("dark");
  return isDark ? MONACO_THEME_DARK : MONACO_THEME_LIGHT;
}

/** 从 Content-Type 推断 Monaco 语言 id。 */
export function languageFromContentType(contentType?: string | null): string {
  if (!contentType) return "plaintext";
  const value = contentType.toLowerCase();
  if (value.includes("json")) return "json";
  if (value.includes("html")) return "html";
  if (value.includes("xml")) return "xml";
  if (value.includes("javascript")) return "javascript";
  if (value.includes("typescript")) return "typescript";
  if (value.includes("css")) return "css";
  if (value.includes("markdown")) return "markdown";
  if (value.includes("yaml")) return "yaml";
  if (value.includes("graphql")) return "graphql";
  if (value.includes("x-www-form-urlencoded")) return "plaintext";
  return "plaintext";
}

/** 猜测一段文本的语言（用于 JSON / XML / HTML 自动高亮）。 */
export function detectLanguage(text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return "plaintext";
  if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
    try {
      JSON.parse(trimmed);
      return "json";
    } catch {
      return "plaintext";
    }
  }
  if (trimmed.startsWith("<!DOCTYPE") || trimmed.startsWith("<html")) return "html";
  if (trimmed.startsWith("<")) return "xml";
  return "plaintext";
}

export interface CompletionVariable {
  key: string;
  source: string;
  secret?: boolean;
}

let variableCompletionRegistered = false;

/**
 * 在 Monaco 中注册 `{{变量}}` 补全（Body 编辑器用）。
 *
 * 通过 getter 惰性读取当前工作区变量，因此工作区/环境变化后无需重新注册。
 */
export function ensureVariableCompletion(
  getVariables: () => CompletionVariable[],
) {
  if (variableCompletionRegistered) return;
  variableCompletionRegistered = true;

  monaco.languages.registerCompletionItemProvider(
    ["json", "plaintext", "xml", "html", "javascript", "yaml"],
    {
      triggerCharacters: ["{"],
      provideCompletionItems(model, position) {
        const before = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });
        const query = findVariableQuery(before, before.length);
        if (!query) return { suggestions: [] };

        // 替换范围覆盖已输入的 `{{prefix`，插入完整的 `{{key}}`。
        const startColumn = Math.max(1, position.column - (query.query.length + 2));
        const range = new monaco.Range(
          position.lineNumber,
          startColumn,
          position.lineNumber,
          position.column,
        );

        return {
          suggestions: getVariables().map((variable) => ({
            label: variable.key,
            kind: monaco.languages.CompletionItemKind.Variable,
            detail: variable.secret ? `${variable.source}（机密）` : variable.source,
            documentation: `插入 {{${variable.key}}}`,
            insertText: `{{${variable.key}}}`,
            filterText: variable.key,
            range,
          })),
        };
      },
    },
  );
}
