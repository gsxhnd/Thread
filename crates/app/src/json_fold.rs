//! JSON 请求体的代码折叠。
//!
//! 编辑器的折叠依赖语法树；这里没有编译 tree-sitter 的 JSON 语法，所以用一个
//! 轻量的括号配对扫描给出折叠范围（跨行的 `{}` / `[]`），字符串里的括号忽略。

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::input::{
    EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter,
    InputHighlighterFactory, Rope,
};
use gpui_kit::{Context, HighlightStyle, SharedString, Window};

/// 只负责折叠，不做语法着色；文字仍用编辑器的默认前景色。
#[derive(Default)]
struct JsonFolds;

impl InputHighlighter for JsonFolds {
    fn language(&self) -> SharedString {
        "json".into()
    }

    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        _text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        // 折叠范围在 `fold_ranges` 里按当前文本重新计算，这里不需要缓存。
    }

    fn styles(
        &self,
        range: &Range<usize>,
        _resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        vec![(range.clone(), HighlightStyle::default())]
    }

    fn fold_ranges(&self, text: &Rope) -> Vec<FoldRange> {
        json_fold_ranges(&text.to_string())
    }
}

/// 编辑器的 JSON 高亮工厂。只为 `json` 提供折叠，其它语言交回默认实现。
pub fn factory() -> InputHighlighterFactory {
    Rc::new(|language: &str| -> Option<Box<dyn InputHighlighter>> {
        if language.eq_ignore_ascii_case("json") {
            Some(Box::new(JsonFolds))
        } else {
            None
        }
    })
}

/// 扫描括号，返回跨至少三行的 `{}` / `[]` 折叠范围（起止行号从 0 开始）。
pub fn json_fold_ranges(text: &str) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut line = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            } else if ch == '\n' {
                // 字符串里不该出现换行；遇到就结束字符串，避免残缺输入打乱后面的配对。
                in_string = false;
                line += 1;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '\n' => line += 1,
            '{' | '[' => stack.push(line),
            '}' | ']' => {
                if let Some(start) = stack.pop()
                    && line >= start + 2
                {
                    ranges.push(FoldRange::new(start, line));
                }
            }
            _ => {}
        }
    }

    ranges.sort_by_key(|range| range.start_line);
    ranges.dedup_by_key(|range| range.start_line);
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<(usize, usize)> {
        json_fold_ranges(text)
            .into_iter()
            .map(|range| (range.start_line, range.end_line))
            .collect()
    }

    #[test]
    fn folds_multi_line_objects_and_arrays() {
        let text = "{\n  \"a\": [\n    1,\n    2\n  ]\n}";
        assert_eq!(spans(text), vec![(0, 5), (1, 4)]);
    }

    #[test]
    fn ignores_brackets_inside_strings() {
        let text = "{\n  \"a\": \"{[\",\n  \"b\": 1\n}";
        assert_eq!(spans(text), vec![(0, 3)]);
    }

    #[test]
    fn single_line_has_no_fold() {
        assert!(spans("{\"a\": [1, 2]}").is_empty());
    }
}
