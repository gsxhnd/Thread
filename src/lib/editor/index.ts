import { useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import CharacterCount from "@tiptap/extension-character-count";
import Placeholder from "@tiptap/extension-placeholder";
import { Markdown } from "tiptap-markdown";

export function useNovelEditor(
  initialMarkdown: string,
  onUpdate?: (markdown: string) => void,
) {
  return useEditor({
    extensions: [
      StarterKit,
      Markdown.configure({
        html: false,
        transformPastedText: true,
        transformCopiedText: true,
      }),
      CharacterCount,
      Placeholder.configure({
        placeholder: "开始书写……",
      }),
    ],
    content: initialMarkdown,
    editorProps: {
      attributes: {
        class:
          "prose prose-neutral dark:prose-invert max-w-none min-h-[60vh] focus:outline-none px-8 py-6 text-base leading-8",
      },
    },
    onUpdate: ({ editor }) => {
      const md = editor.storage.markdown.getMarkdown() as string;
      onUpdate?.(md);
    },
  });
}

export function getMarkdown(editor: {
  storage: { markdown: { getMarkdown: () => string } };
}): string {
  return editor.storage.markdown.getMarkdown();
}
