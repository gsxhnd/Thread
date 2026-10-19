//! 请求地址里的 `{{变量}}`：补全当前环境的变量名，并把写完整的变量画成色块。

use std::ops::Range;

use gpui_kit::component::input::{InlineToken, Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::workspace::index::WorkspaceIndex;

use crate::app::AppView;

/// 地址栏补全的选中项。只在用户正在输入 `{{` 时有意义。
pub struct UrlAssist {
    pub selected: usize,
    partial: String,
    syncing: bool,
}

impl Default for UrlAssist {
    fn default() -> Self {
        Self {
            selected: 0,
            partial: String::new(),
            syncing: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VarKey {
    name: String,
    secret: bool,
}

struct VarSpan {
    start: usize,
    end: usize,
    key: String,
}

/// 订阅地址变化。订阅挂在标签页上，关掉标签就一起释放。
pub fn watch_request_url(
    this: &mut AppView,
    url: Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let subscription = cx.subscribe_in(
        &url,
        window,
        |this, input, event: &InputEvent, window, cx| {
            if this.url_assist.syncing {
                return;
            }
            let Some(tab) = this.tabs.iter().position(|tab| tab.url == *input) else {
                return;
            };
            match event {
                InputEvent::PressEnter { .. } => {
                    if this.active_tab == Some(tab) {
                        this.accept_url_variable(window, cx);
                    }
                }
                InputEvent::Change | InputEvent::Focus => {
                    this.refresh_url_variables(tab, window, cx);
                }
                InputEvent::Blur => {}
            }
        },
    );
    if let Some(tab) = this.tabs.iter_mut().find(|tab| tab.url == url) {
        tab.url_watch = Some(subscription);
    }
    if let Some(tab) = this.tabs.iter().position(|tab| tab.url == url) {
        this.refresh_url_variables(tab, window, cx);
    }
}

/// 工作区变量变了之后，重画已经打开的地址。
pub fn resync_open_urls(this: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    let count = this.tabs.len();
    for index in 0..count {
        this.refresh_url_variables(index, window, cx);
    }
}

impl AppView {
    fn refresh_url_variables(&mut self, tab: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.url_assist.syncing {
            return;
        }
        let Some(url) = self.tabs.get(tab).map(|tab| tab.url.clone()) else {
            return;
        };
        let keys = variable_keys(self.workspace.read(cx).index());
        self.url_assist.syncing = true;
        url.update(cx, |state, cx| {
            let cursor = state.cursor();
            if sync_tokens(state, &keys, window, cx) {
                state.set_selected_range(cursor..cursor, cx);
            }
        });
        self.url_assist.syncing = false;

        let value = url.read(cx).value();
        let cursor = url.read(cx).cursor();
        match open_partial(value.as_ref(), cursor) {
            Some((_, partial)) if self.active_tab == Some(tab) => {
                if partial != self.url_assist.partial {
                    self.url_assist.selected = 0;
                    self.url_assist.partial = partial;
                }
            }
            _ => {
                if self.active_tab == Some(tab) {
                    self.url_assist.partial.clear();
                    self.url_assist.selected = 0;
                }
            }
        }
        cx.notify();
    }

    fn accept_url_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab else {
            return;
        };
        let Some(url) = self.tabs.get(tab).map(|tab| tab.url.clone()) else {
            return;
        };
        let keys = variable_keys(self.workspace.read(cx).index());
        let value = url.read(cx).value();
        let cursor = url.read(cx).cursor();
        let Some((range, partial)) = open_partial(value.as_ref(), cursor) else {
            return;
        };
        let matched = matching_keys(&keys, &partial);
        let Some(name) = matched
            .get(self.url_assist.selected)
            .map(|key| key.name.clone())
        else {
            return;
        };
        let body = braced_name(&name);
        let token = InlineToken::new(format!("var:{name}"), body);
        self.url_assist.syncing = true;
        let mut inserted = false;
        url.update(cx, |state, cx| {
            inserted = state
                .replace_range_with_token(range, token, window, cx)
                .is_ok();
        });
        self.url_assist.syncing = false;
        if inserted {
            self.url_assist.partial.clear();
            self.url_assist.selected = 0;
        }
        cx.notify();
    }
}

/// 地址输入框。当前标签正在输入 `{{` 时，下面列出能补上的变量名。
pub fn field(
    this: &mut AppView,
    tab: usize,
    url: &Entity<InputState>,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let menu = if this.active_tab == Some(tab) {
        suggestion_menu(this, url, cx)
    } else {
        None
    };
    v_flex()
        .flex_1()
        .min_w(px(160.))
        .gap_1()
        .child(div().h(px(28.)).child(styled_input(url)))
        .when_some(menu, |row, menu| row.child(menu))
        .into_any_element()
}

fn styled_input(url: &Entity<InputState>) -> Input {
    Input::new(url).small().token(|token, _, cx| {
        let theme = cx.theme().clone();
        let known = token.token().id().starts_with("var:");
        let color = if known { theme.primary } else { theme.warning };
        div()
            .px_0p5()
            .rounded_sm()
            .text_color(color)
            .bg(color.opacity(0.16))
            .child(token.token().label().clone())
    })
}

fn suggestion_menu(
    this: &mut AppView,
    url: &Entity<InputState>,
    cx: &mut Context<AppView>,
) -> Option<AnyElement> {
    let value = url.read(cx).value();
    let cursor = url.read(cx).cursor();
    let (_, partial) = open_partial(value.as_ref(), cursor)?;
    let keys = variable_keys(this.workspace.read(cx).index());
    let matched = matching_keys(&keys, &partial);
    if matched.is_empty() {
        return None;
    }
    let selected = this.url_assist.selected.min(matched.len() - 1);
    let theme = cx.theme().clone();
    let rows = matched.iter().enumerate().map(|(ix, key)| {
        let name = key.name.clone();
        let secret = key.secret;
        let on = ix == selected;
        h_flex()
            .id(SharedString::from(format!("url-var-{ix}")))
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .cursor_pointer()
            .when(on, |row| row.bg(theme.accent))
            .when(!on, |row| row.hover(|row| row.bg(theme.muted)))
            .child(div().text_sm().child(name))
            .when(secret, |row| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tr!(cx, "机密", "Secret")),
                )
            })
            .on_click(
                cx.listener(move |this, _: &gpui_kit::ClickEvent, window, cx| {
                    this.url_assist.selected = ix;
                    this.accept_url_variable(window, cx);
                }),
            )
            .into_any_element()
    });

    Some(
        v_flex()
            .w_full()
            .max_h(px(220.))
            .id("url-var-suggestions")
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .py_1()
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(cx, "环境变量", "Environment variables")),
            )
            .children(rows)
            .into_any_element(),
    )
}

fn variable_keys(index: Option<&WorkspaceIndex>) -> Vec<VarKey> {
    let Some(index) = index else {
        return Vec::new();
    };
    let mut keys = Vec::new();
    for environment in index.environments.iter().filter(|env| env.is_active) {
        push_keys(&mut keys, environment);
    }
    for environment in index
        .environments
        .iter()
        .filter(|env| env.is_global && !env.is_active)
    {
        push_keys(&mut keys, environment);
    }
    if keys.is_empty() {
        for environment in &index.environments {
            push_keys(&mut keys, environment);
        }
    }
    keys.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    keys
}

fn push_keys(
    keys: &mut Vec<VarKey>,
    environment: &thread_core::workspace::index::EnvironmentSummary,
) {
    for name in &environment.variable_names {
        if keys.iter().any(|key| key.name == *name) {
            continue;
        }
        let secret = environment
            .secret_variable_names
            .iter()
            .any(|candidate| candidate == name);
        keys.push(VarKey {
            name: name.clone(),
            secret,
        });
    }
}

fn matching_keys<'a>(keys: &'a [VarKey], partial: &str) -> Vec<&'a VarKey> {
    let needle = partial.to_ascii_lowercase();
    keys.iter()
        .filter(|key| key.name.to_ascii_lowercase().starts_with(&needle))
        .collect()
}

fn sync_tokens(
    state: &mut InputState,
    keys: &[VarKey],
    window: &mut Window,
    cx: &mut Context<InputState>,
) -> bool {
    let text = state.value().to_string();
    let spans = complete_variables(&text);
    let existing: Vec<_> = state
        .tokens()
        .iter()
        .map(|token| (token.range(), token.token().id().to_string()))
        .collect();
    let mut changed = false;
    for span in spans.into_iter().rev() {
        let known = keys.iter().any(|key| key.name == span.key);
        let id = if known {
            format!("var:{}", span.key)
        } else {
            format!("missing:{}", span.key)
        };
        let already = existing
            .iter()
            .any(|(range, token_id)| *range == (span.start..span.end) && token_id == &id);
        if already {
            continue;
        }
        let body = text[span.start..span.end].to_string();
        let token = InlineToken::new(id, body);
        if state
            .replace_range_with_token(span.start..span.end, token, window, cx)
            .is_ok()
        {
            changed = true;
        }
    }
    changed
}

fn complete_variables(text: &str) -> Vec<VarSpan> {
    let mut spans = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 3 < bytes.len() {
        if bytes[index] == b'{' && bytes[index + 1] == b'{' {
            if let Some(end_rel) = text[index + 2..].find("}}") {
                let key = &text[index + 2..index + 2 + end_rel];
                if is_key(key) {
                    let end = index + 2 + end_rel + 2;
                    spans.push(VarSpan {
                        start: index,
                        end,
                        key: key.to_string(),
                    });
                    index = end;
                    continue;
                }
            }
        }
        index += text[index..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(1);
    }
    spans
}

fn open_partial(text: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    if !text.is_char_boundary(cursor) {
        return None;
    }
    let head = &text[..cursor];
    let start = head.rfind("{{")?;
    let partial = &head[start + 2..];
    if partial.contains(['{', '}']) || partial.chars().any(char::is_whitespace) {
        return None;
    }
    if !partial.chars().all(is_key_char) {
        return None;
    }
    Some((start..cursor, partial.to_string()))
}

fn is_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(is_key_char)
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

fn braced_name(name: &str) -> String {
    format!("{{{{{name}}}}}")
}

#[cfg(test)]
mod tests {
    use super::{VarKey, braced_name, complete_variables, is_key, matching_keys, open_partial};

    fn key(name: &str) -> VarKey {
        VarKey {
            name: name.to_string(),
            secret: false,
        }
    }

    #[test]
    fn complete_variable_spans_keep_the_braces() {
        let text = "https://{{baseUrl}}/v1/{{user-id}}";
        let spans = complete_variables(text);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].key, "baseUrl");
        assert_eq!(&text[spans[0].start..spans[0].end], "{{baseUrl}}");
        assert_eq!(spans[1].key, "user-id");
        assert_eq!(&text[spans[1].start..spans[1].end], "{{user-id}}");
    }

    #[test]
    fn unfinished_braces_are_not_a_variable() {
        let spans = complete_variables("https://{{base");
        assert!(spans.is_empty());
    }

    #[test]
    fn partial_starts_at_the_open_braces() {
        let text = "https://{{ba";
        let (range, partial) = open_partial(text, text.len()).expect("partial");
        assert_eq!(partial, "ba");
        assert_eq!(&text[range], "{{ba");
    }

    #[test]
    fn closed_variable_does_not_keep_the_menu_open() {
        let text = "https://{{baseUrl}}/users";
        assert!(open_partial(text, text.len()).is_none());
    }

    #[test]
    fn empty_partial_lists_every_key() {
        let keys = vec![key("baseUrl"), key("token")];
        assert_eq!(matching_keys(&keys, "").len(), 2);
    }

    #[test]
    fn prefix_match_is_case_insensitive() {
        let keys = vec![key("baseUrl"), key("basePath"), key("token")];
        let matched = matching_keys(&keys, "BASE");
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].name, "baseUrl");
    }

    #[test]
    fn blank_name_is_not_a_key() {
        assert!(!is_key(""));
        assert!(is_key("baseUrl"));
    }

    #[test]
    fn braced_name_wraps_the_key() {
        assert_eq!(braced_name("baseUrl"), "{{baseUrl}}");
    }
}
