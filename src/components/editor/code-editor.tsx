import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import {
  MONACO_THEME_DARK,
  MONACO_THEME_LIGHT,
  ensureMonacoThemes,
  monaco,
} from "@/lib/editor/monaco";

type CodeEditorProps = {
  value: string;
  language?: string;
  readOnly?: boolean;
  onChange?: (value: string) => void;
  className?: string;
  /** 最小高度，默认 220px。 */
  minHeight?: number;
  /** 只读模式下自动换行（响应体展示用）。 */
  wordWrap?: boolean;
};

const options: monaco.editor.IStandaloneEditorConstructionOptions = {
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  fontSize: 13,
  lineNumbersMinChars: 3,
  renderLineHighlight: "none",
  automaticLayout: true,
  tabSize: 2,
  scrollbar: { alwaysConsumeMouseWheel: false },
  padding: { top: 8, bottom: 8 },
  fixedOverflowWidgets: true,
};

/**
 * Monaco 编辑器封装（本地加载，无 CDN）。
 *
 * - 通过 `document.documentElement.classList` 跟随 next-themes 明暗切换；
 * - 只读模式用于展示响应体 / 原始文档；
 * - 组件卸载时 dispose，避免长会话内存泄漏。
 */
export function CodeEditor({
  value,
  language = "plaintext",
  readOnly = false,
  onChange,
  className,
  minHeight = 220,
  wordWrap = false,
}: CodeEditorProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    ensureMonacoThemes();
    const container = containerRef.current;
    if (!container) return;

    const editor = monaco.editor.create(container, {
      ...options,
      value,
      language,
      readOnly,
      theme: MONACO_THEME_LIGHT,
      wordWrap: wordWrap ? "on" : "off",
    });
    editorRef.current = editor;

    const subscription = editor.onDidChangeModelContent(() => {
      onChangeRef.current?.(editor.getValue());
    });

    const applyTheme = () => {
      monaco.editor.setTheme(
        document.documentElement.classList.contains("dark")
          ? MONACO_THEME_DARK
          : MONACO_THEME_LIGHT,
      );
    };
    applyTheme();
    const observer = new MutationObserver(applyTheme);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => {
      observer.disconnect();
      subscription.dispose();
      editor.getModel()?.dispose();
      editor.dispose();
      editorRef.current = null;
    };
    // 编辑器实例只在挂载时创建；后续变更通过下方 effect 同步。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 外部值变化（切换标签页 / 重置草稿）时同步到编辑器。
  useEffect(() => {
    const editor = editorRef.current;
    if (!editor) return;
    if (editor.getValue() !== value) {
      editor.setValue(value);
    }
  }, [value]);

  useEffect(() => {
    editorRef.current?.updateOptions({ readOnly, wordWrap: wordWrap ? "on" : "off" });
  }, [readOnly, wordWrap]);

  useEffect(() => {
    const model = editorRef.current?.getModel();
    if (model && model.getLanguageId() !== language) {
      monaco.editor.setModelLanguage(model, language);
    }
  }, [language]);

  return (
    <div
      ref={containerRef}
      data-slot="code-editor"
      className={cn("w-full overflow-hidden rounded-lg border border-border", className)}
      style={{ minHeight }}
    />
  );
}
