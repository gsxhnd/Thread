//! 请求地址里的 `{{变量}}`：补全当前环境的变量名，并把写完整的变量画成色块。

use std::collections::BTreeMap;
use std::ops::Range;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::input::{InlineToken, Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window, deferred, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::services::http::HttpRequestConfig;
use thread_core::workspace::formats::{AuthSpec, HttpBody, HttpBodyField, KeyValue};
use thread_core::workspace::index::{EnvironmentSummary, WorkspaceIndex};

use crate::app::AppView;

/// 变量写在哪个环境文件里，以及它是不是机密。
#[derive(Clone, Debug)]
struct VarHome {
    path: String,
    secret: bool,
}

/// 地址栏补全的选中项。只在用户正在输入 `{{` 时有意义。
pub struct UrlAssist {
    pub selected: usize,
    partial: String,
    syncing: bool,
    /// 当前环境的有效变量。工作区索引变了才重新读。
    values: BTreeMap<String, String>,
    homes: BTreeMap<String, VarHome>,
    stamp: String,
    /// 悬停变量时用来改值的输入框。
    editor: Entity<InputState>,
    editing: Option<String>,
}

impl UrlAssist {
    pub fn new(editor: Entity<InputState>) -> Self {
        Self {
            selected: 0,
            partial: String::new(),
            syncing: false,
            values: BTreeMap::new(),
            homes: BTreeMap::new(),
            stamp: String::new(),
            editor,
            editing: None,
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
    /// 读出当前环境里生效的变量。索引没变就用上次的结果。
    pub fn current_variables(&mut self, cx: &App) -> BTreeMap<String, String> {
        self.ensure_variables(cx);
        self.url_assist.values.clone()
    }

    fn ensure_variables(&mut self, cx: &App) {
        let Some(index) = self.workspace.read(cx).index().cloned() else {
            self.url_assist.values.clear();
            self.url_assist.homes.clear();
            self.url_assist.stamp.clear();
            return;
        };
        if self.url_assist.stamp == index.digest {
            return;
        }
        let root = std::path::PathBuf::from(&index.root);
        let active = index
            .environments
            .iter()
            .find(|environment| environment.is_active)
            .map(|environment| environment.path.clone());
        let values = thread_core::workspace::ops::resolve_variables(&root, active.as_deref())
            .unwrap_or_default();
        let mut homes = BTreeMap::new();
        for environment in index
            .environments
            .iter()
            .filter(|environment| !environment.is_active && !environment.is_global)
        {
            remember_home(&mut homes, environment);
        }
        for environment in index
            .environments
            .iter()
            .filter(|environment| environment.is_global && !environment.is_active)
        {
            remember_home(&mut homes, environment);
        }
        for environment in index
            .environments
            .iter()
            .filter(|environment| environment.is_active)
        {
            remember_home(&mut homes, environment);
        }
        self.url_assist.values = values;
        self.url_assist.homes = homes;
        self.url_assist.stamp = index.digest;
    }

    fn save_url_variable(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_variables(cx);
        let value = self.url_assist.editor.read(cx).value().to_string();
        let Some(index) = self.workspace.read(cx).index().cloned() else {
            return;
        };
        let Some((path, secret)) = variable_destination(&index, key, &self.url_assist.homes) else {
            window.push_notification(
                tr!(
                    cx,
                    "还没有环境，先建一个再改变量。",
                    "Create an environment before editing variables."
                ),
                cx,
            );
            return;
        };
        let key = key.to_string();
        self.workspace.update(cx, |model, cx| {
            model.run(cx, move |root| {
                thread_core::workspace::ops::set_env_variable(
                    &root,
                    &path,
                    &key,
                    Some(value),
                    Some(true),
                    Some(secret),
                    None,
                )
                .map(|_| String::new())
            });
        });
    }

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

/// 地址输入框。当前标签正在输入 `{{` 时，在输入框下方浮出能补上的变量名。
pub fn field(
    this: &mut AppView,
    tab: usize,
    url: &Entity<InputState>,
    cx: &mut Context<AppView>,
) -> AnyElement {
    this.ensure_variables(cx);
    let values = this.url_assist.values.clone();
    let menu = if this.active_tab == Some(tab) {
        suggestion_menu(this, url, cx)
    } else {
        None
    };
    div()
        .flex_1()
        .min_w(px(160.))
        .relative()
        .child(styled_input(url, values))
        .when_some(menu, |row, menu| {
            row.child(
                deferred(
                    div()
                        .absolute()
                        .top(px(32.))
                        .left_0()
                        .right_0()
                        .occlude()
                        .child(menu),
                )
                .with_priority(2),
            )
        })
        .into_any_element()
}

fn styled_input(url: &Entity<InputState>, values: BTreeMap<String, String>) -> Input {
    Input::new(url).small().token(move |token, _, cx| {
        let theme = cx.theme().clone();
        let id = token.token().id().to_string();
        let key = id
            .strip_prefix("var:")
            .or_else(|| id.strip_prefix("missing:"))
            .unwrap_or("")
            .to_string();
        let known = id.starts_with("var:");
        let color = if known { theme.primary } else { theme.warning };
        let chip = div()
            .px_0p5()
            .rounded_sm()
            .text_color(color)
            .bg(color.opacity(0.16))
            .child(token.token().label().clone());
        if key.is_empty() {
            return chip.into_any_element();
        }
        let dynamic = key.starts_with('$');
        let shown = values.get(&key).cloned();
        let open_key = key.clone();
        HoverCard::new(SharedString::from(format!("url-var-{key}")))
            .anchor(gpui_kit::Anchor::TopLeft)
            .open_delay(Duration::from_millis(250))
            .close_delay(Duration::from_millis(400))
            .trigger(chip)
            .on_open_change(move |open, window, cx| {
                if *open && !dynamic {
                    begin_var_edit(&open_key, window, cx);
                }
            })
            .content({
                let key = key.clone();
                let shown = shown.clone();
                move |_, _, cx| var_card(&key, dynamic, shown.clone(), cx)
            })
            .into_any_element()
    })
}

fn begin_var_edit(key: &str, window: &mut Window, cx: &mut App) {
    let Some(app) = cx.try_global::<crate::app::MainApp>() else {
        return;
    };
    let Some(view) = app.0.upgrade() else {
        return;
    };
    let key = key.to_string();
    view.update(cx, |this, cx| {
        this.ensure_variables(cx);
        let value = this
            .url_assist
            .values
            .get(&key)
            .cloned()
            .unwrap_or_default();
        this.url_assist.editing = Some(key);
        let editor = this.url_assist.editor.clone();
        editor.update(cx, |state, cx| {
            state.set_value(value, window, cx);
        });
    });
}

fn var_card(key: &str, dynamic: bool, shown: Option<String>, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let editor = cx
        .try_global::<crate::app::MainApp>()
        .and_then(|app| app.0.upgrade())
        .map(|view| view.read(cx).url_assist.editor.clone());
    let defined = shown.is_some();
    let value = if dynamic {
        tr!(cx, "发送时生成", "Generated when the request is sent").to_string()
    } else {
        shown.unwrap_or_else(|| tr!(cx, "未定义", "Undefined").to_string())
    };
    v_flex()
        .w(px(260.))
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(key.to_string()),
        )
        .child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(if defined && !dynamic {
                    theme.foreground
                } else if dynamic {
                    theme.muted_foreground
                } else {
                    theme.warning
                })
                .child(value),
        )
        .when(!dynamic, |card| {
            card.when_some(editor, |card, editor| {
                let save_key = key.to_string();
                card.child(Input::new(&editor).small()).child(
                    Button::new(SharedString::from(format!("url-var-save-{key}")))
                        .xsmall()
                        .primary()
                        .label(tr!(cx, "保存", "Save"))
                        .on_click(move |_, window, cx| save_var_edit(&save_key, window, cx)),
                )
            })
        })
        .into_any_element()
}

fn save_var_edit(key: &str, window: &mut Window, cx: &mut App) {
    let Some(app) = cx.try_global::<crate::app::MainApp>() else {
        return;
    };
    let Some(view) = app.0.upgrade() else {
        return;
    };
    let key = key.to_string();
    view.update(cx, |this, cx| this.save_url_variable(&key, window, cx));
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

fn remember_home(homes: &mut BTreeMap<String, VarHome>, environment: &EnvironmentSummary) {
    for name in &environment.variable_names {
        let secret = environment
            .secret_variable_names
            .iter()
            .any(|candidate| candidate == name);
        homes.insert(
            name.clone(),
            VarHome {
                path: environment.path.clone(),
                secret,
            },
        );
    }
}

fn variable_destination(
    index: &WorkspaceIndex,
    key: &str,
    homes: &BTreeMap<String, VarHome>,
) -> Option<(String, bool)> {
    if let Some(home) = homes.get(key) {
        return Some((home.path.clone(), home.secret));
    }
    let environment = index
        .environments
        .iter()
        .find(|environment| environment.is_active)
        .or_else(|| {
            index
                .environments
                .iter()
                .find(|environment| environment.is_global)
        })?;
    Some((environment.path.clone(), false))
}

/// 把请求里的 `{{变量}}` 换成当前环境的值。未知变量保持原样。
pub fn concrete_config(
    config: &HttpRequestConfig,
    values: &BTreeMap<String, String>,
) -> HttpRequestConfig {
    if !has_template(config) {
        return config.clone();
    }
    let sub = |text: &str| -> String {
        let mut missing = Vec::new();
        thread_core::services::variables::substitute(text, values, &mut missing)
    };
    HttpRequestConfig {
        method: config.method.clone(),
        url: sub(&config.url),
        query_params: config
            .query_params
            .iter()
            .map(|row| KeyValue {
                key: sub(&row.key),
                value: sub(&row.value),
                ..row.clone()
            })
            .collect(),
        headers: config
            .headers
            .iter()
            .map(|row| KeyValue {
                key: sub(&row.key),
                value: sub(&row.value),
                ..row.clone()
            })
            .collect(),
        body: HttpBody {
            content: sub(&config.body.content),
            fields: config
                .body
                .fields
                .iter()
                .map(|field| HttpBodyField {
                    key: sub(&field.key),
                    value: sub(&field.value),
                    file_path: field.file_path.as_ref().map(|path| sub(path)),
                    ..field.clone()
                })
                .collect(),
            ..config.body.clone()
        },
        auth: AuthSpec {
            token: config.auth.token.as_ref().map(|value| sub(value)),
            username: config.auth.username.as_ref().map(|value| sub(value)),
            password: config.auth.password.as_ref().map(|value| sub(value)),
            key: config.auth.key.as_ref().map(|value| sub(value)),
            value: config.auth.value.as_ref().map(|value| sub(value)),
            ..config.auth.clone()
        },
        settings: config.settings.clone(),
    }
}

fn has_template(config: &HttpRequestConfig) -> bool {
    let marked = |text: &str| text.contains("{{");
    marked(&config.url)
        || config
            .query_params
            .iter()
            .any(|row| marked(&row.key) || marked(&row.value))
        || config
            .headers
            .iter()
            .any(|row| marked(&row.key) || marked(&row.value))
        || marked(&config.body.content)
        || config.body.fields.iter().any(|field| {
            marked(&field.key)
                || marked(&field.value)
                || field.file_path.as_deref().is_some_and(marked)
        })
        || config.auth.token.as_deref().is_some_and(marked)
        || config.auth.username.as_deref().is_some_and(marked)
        || config.auth.password.as_deref().is_some_and(marked)
        || config.auth.key.as_deref().is_some_and(marked)
        || config.auth.value.as_deref().is_some_and(marked)
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
    use std::collections::BTreeMap;

    use thread_core::services::http::HttpRequestConfig;
    use thread_core::workspace::formats::KeyValue;

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

    #[test]
    fn concrete_config_fills_known_variables_and_keeps_missing_ones() {
        let mut values = BTreeMap::new();
        values.insert("base".to_string(), "https://example.com".to_string());
        let mut config = HttpRequestConfig::default();
        config.url = "{{base}}/users/{{id}}".into();
        config.headers.push(KeyValue::new("X-Host", "{{base}}"));
        let shown = super::concrete_config(&config, &values);
        assert_eq!(shown.url, "https://example.com/users/{{id}}");
        assert_eq!(shown.headers[0].value, "https://example.com");
        let again = super::concrete_config(&shown, &values);
        assert_eq!(again.url, shown.url);
    }
}
