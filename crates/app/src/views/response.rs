//! 响应面板：状态行 + 响应体/响应头/Cookie/耗时。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, IntoElement, ParentElement as _, SharedString, Styled,
    Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};

use crate::app::{AppView, RequestTab};
use crate::format::{format_bytes, format_duration};
use crate::widgets;

/// 响应展示分区。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResponseSection {
    Body,
    Headers,
    Cookies,
    Timing,
    Console,
}

impl ResponseSection {
    /// 分栏的展示顺序。
    pub const ALL: [Self; 5] = [
        Self::Body,
        Self::Headers,
        Self::Cookies,
        Self::Timing,
        Self::Console,
    ];

    fn label(self, cx: &App) -> &'static str {
        match self {
            Self::Body => tr!(cx, "响应体", "Body"),
            Self::Headers => tr!(cx, "响应头", "Headers"),
            Self::Cookies => "Cookie",
            Self::Timing => tr!(cx, "耗时", "Timing"),
            Self::Console => tr!(cx, "控制台", "Console"),
        }
    }
}

/// 响应体按哪种格式展示。`Auto` 跟随 Content-Type，下拉里可以改成指定格式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ResponseBodyMode {
    #[default]
    Auto,
    Json,
    Xml,
    Html,
    Yaml,
    Raw,
}

impl ResponseBodyMode {
    const CHOICES: [Self; 5] = [Self::Json, Self::Xml, Self::Html, Self::Yaml, Self::Raw];

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Json => "JSON",
            Self::Xml => "XML",
            Self::Html => "HTML",
            Self::Yaml => "YAML",
            Self::Raw => "Raw",
        }
    }
}

/// 响应到达后把分栏切回响应体。没有响应时不动。
pub fn auto_select_section(tab: &mut RequestTab) {
    if tab.response.is_some() {
        tab.response_section = ResponseSection::Body;
    }
}

/// 分栏标签的基础样式：选中态有底线并加粗，未选中态是弱化文字。
fn tab_style<E: Styled + FluentBuilder>(element: E, selected: bool, theme: &Theme) -> E {
    element
        .px_2()
        .py_1()
        .text_xs()
        .border_b_2()
        .when(selected, |this| {
            this.border_color(theme.primary)
                .text_color(theme.foreground)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!selected, |this| {
            this.border_color(gpui_kit::transparent_black())
                .text_color(theme.muted_foreground)
        })
}

pub fn render(
    this: &mut AppView,
    tab_ix: usize,
    _window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(tab) = this.tabs.get(tab_ix) else {
        return empty_response(false, cx);
    };
    let sending = tab.sending;
    let Some(response) = &tab.response else {
        return empty_response(sending, cx);
    };
    let section = tab.response_section;
    let body_mode = tab.response_body_mode;
    let timing = &response.timing;
    let unresolved = &response.unresolved_variables;

    let section_bar = h_flex()
        .gap_1()
        .children(ResponseSection::ALL.map(|candidate| {
            let selected = section == candidate;
            tab_style(div(), selected, &theme)
                .id(SharedString::from(format!("resp-section-{candidate:?}")))
                .cursor_pointer()
                .when(!selected, |this| {
                    this.hover(|this| this.text_color(theme.foreground))
                })
                .child(response_section_label(candidate, &response.headers, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(tab) = this.tabs.get_mut(tab_ix) {
                        tab.response_section = candidate;
                        cx.notify();
                    }
                }))
        }));

    let content = match section {
        ResponseSection::Body => body_view(
            tab_ix,
            response.body.as_deref(),
            response.body_file_path.as_deref(),
            response.content_type.as_deref(),
            body_mode,
            cx,
        ),
        ResponseSection::Headers => headers_view(&response.headers, cx),
        ResponseSection::Cookies => cookies_view(&response.headers, cx),
        ResponseSection::Timing => timing_view(timing, cx),
        ResponseSection::Console => console_view(response, cx),
    };

    v_flex()
        .size_full()
        .min_h_0()
        .gap_2()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .p_3()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .justify_between()
                .flex_wrap()
                .gap_y_1()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(widgets::StatusTag::new(
                            response.status_code,
                            Some(&response.status_text),
                            cx,
                        ))
                        .child(widgets::mini_tag(cx, format_bytes(response.body_size)))
                        .child(widgets::mini_tag(
                            cx,
                            format_duration(response.timing.total_ms),
                        ))
                        .when_some(response.content_type.clone(), |row, content_type| {
                            row.child(widgets::mini_tag(cx, content_type_label(&content_type)))
                        })
                        .when(response.redirect_count > 0, |this| {
                            this.child(widgets::mini_tag(
                                cx,
                                tr!(
                                    cx,
                                    format!("{} 次重定向", response.redirect_count),
                                    format!("{} redirects", response.redirect_count)
                                ),
                            ))
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .text_ellipsis_start()
                        .max_w(px(420.))
                        .child(response.url.clone()),
                ),
        )
        .when(!unresolved.is_empty(), |this| {
            this.child(widgets::notice_line(
                cx,
                &tr!(
                    cx,
                    format!("未定义的变量：{}", unresolved.join("、")),
                    format!("Undefined variables: {}", unresolved.join(", "))
                ),
            ))
        })
        .child(section_bar)
        .child(div().flex_1().min_h_0().child(content))
        .into_any_element()
}

/// 还没有响应时也画出同一块面板，分栏从打开请求起就占好位置。
fn empty_response(sending: bool, cx: &App) -> AnyElement {
    let theme = cx.theme().clone();
    let center = if sending {
        v_flex()
            .gap_3()
            .items_center()
            .child(Spinner::new().large().color(theme.muted_foreground))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(cx, "请求发送中…", "Sending…")),
            )
            .into_any_element()
    } else {
        widgets::compact_empty(
            cx,
            IconName::Inbox,
            tr!(
                cx,
                "发送请求后，响应会显示在这里。",
                "The response shows up here after you send."
            ),
        )
        .into_any_element()
    };
    v_flex()
        .size_full()
        .min_h_0()
        .gap_2()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .p_3()
        .child(response_sections(ResponseSection::Body, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .child(center),
        )
        .into_any_element()
}

/// 只读的分栏条，给空状态用：样式与可点击的分栏条一致，但不响应点击。
fn response_sections(selected: ResponseSection, cx: &App) -> impl IntoElement {
    let theme = cx.theme().clone();
    h_flex()
        .gap_1()
        .children(ResponseSection::ALL.map(|candidate| {
            tab_style(div(), candidate == selected, &theme).child(candidate.label(cx))
        }))
}

/// 响应体支持自动格式化、原始文本和静态预览。
fn body_view(
    tab_ix: usize,
    body: Option<&str>,
    body_file_path: Option<&str>,
    content_type: Option<&str>,
    mode: ResponseBodyMode,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(body) = body else {
        let hint = body_file_path
            .map(|path| {
                tr!(
                    cx,
                    format!("响应体过大，已落盘：{path}"),
                    format!("Response body was saved to {path}")
                )
            })
            .unwrap_or_else(|| {
                tr!(cx, "响应体未内联。", "Response body was not inlined.").to_string()
            });
        return div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(hint)
            .into_any_element();
    };

    let detected = detect_format(body, content_type);
    let selected = match mode {
        ResponseBodyMode::Auto => detected,
        chosen => chosen,
    };
    let (display, parse_error) = render_body(body, selected);
    let mono = theme.mono_font_family.clone();

    v_flex()
        .min_h_0()
        .flex_1()
        .gap_1()
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    Button::new("response-body-format")
                        .outline()
                        .xsmall()
                        .label(selected.label())
                        .dropdown_menu(move |menu, _, _| {
                            let mut menu = menu;
                            for candidate in ResponseBodyMode::CHOICES {
                                let label = if candidate == selected {
                                    format!("✓ {}", candidate.label())
                                } else {
                                    candidate.label().to_string()
                                };
                                menu = menu.item(PopupMenuItem::new(label).on_click(
                                    move |_, _, cx| {
                                        let Some(main) = cx.try_global::<crate::app::MainApp>()
                                        else {
                                            return;
                                        };
                                        let Some(view) = main.0.upgrade() else {
                                            return;
                                        };
                                        view.update(cx, |this, cx| {
                                            if let Some(tab) = this.tabs.get_mut(tab_ix) {
                                                tab.response_body_mode = candidate;
                                                cx.notify();
                                            }
                                        });
                                    },
                                ));
                            }
                            menu
                        }),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .when(parse_error, |row| {
                            row.child(div().text_xs().text_color(theme.muted_foreground).child(
                                tr!(
                                    cx,
                                    "按所选格式解析失败，显示原始文本",
                                    "Could not parse as the selected format; showing raw text"
                                ),
                            ))
                        })
                        .child(
                            Clipboard::new("copy-response-body")
                                .small()
                                .value(display.clone())
                                .tooltip(tr!(cx, "复制当前视图", "Copy current view")),
                        ),
                ),
        )
        .child(code_panel(&display, &mono, &theme))
        .into_any_element()
}

fn detect_format(body: &str, content_type: Option<&str>) -> ResponseBodyMode {
    let mime = content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if mime.contains("json")
        || (mime.is_empty()
            && (body.trim_start().starts_with('{') || body.trim_start().starts_with('[')))
    {
        ResponseBodyMode::Json
    } else if mime.contains("yaml")
        || mime.contains("yml")
        || ((mime.is_empty() || mime == "text/plain") && looks_like_yaml(body))
    {
        ResponseBodyMode::Yaml
    } else if mime.contains("html")
        || body
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype html")
        || body.trim_start().to_ascii_lowercase().starts_with("<html")
    {
        ResponseBodyMode::Html
    } else if mime.contains("xml")
        || (mime.is_empty()
            && (body.trim_start().starts_with("<?xml") || body.trim_start().starts_with('<')))
    {
        ResponseBodyMode::Xml
    } else {
        ResponseBodyMode::Raw
    }
}

fn render_body(body: &str, format: ResponseBodyMode) -> (String, bool) {
    let parsed = match format {
        ResponseBodyMode::Json => pretty_json(body),
        ResponseBodyMode::Xml => pretty_xml(body),
        ResponseBodyMode::Yaml => pretty_yaml(body),
        ResponseBodyMode::Html | ResponseBodyMode::Raw | ResponseBodyMode::Auto => None,
    };
    match format {
        ResponseBodyMode::Html | ResponseBodyMode::Raw | ResponseBodyMode::Auto => {
            (body.to_string(), false)
        }
        ResponseBodyMode::Json | ResponseBodyMode::Xml | ResponseBodyMode::Yaml => match parsed {
            Some(text) => (text, false),
            None => (body.to_string(), true),
        },
    }
}

fn pretty_json(body: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

fn pretty_xml(body: &str) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;
    use quick_xml::writer::Writer;

    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(event) => writer.write_event(event).ok()?,
            Err(_) => return None,
        }
    }
    String::from_utf8(writer.into_inner()).ok()
}

fn looks_like_yaml(body: &str) -> bool {
    let trimmed = body.trim_start();
    if !trimmed.starts_with("---")
        && !trimmed.starts_with("- ")
        && !trimmed.lines().any(|line| {
            let line = line.trim_start();
            line.find(':').is_some_and(|colon| colon > 0)
        })
    {
        return false;
    }
    matches!(
        serde_yaml::from_str::<serde_yaml::Value>(body),
        Ok(serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_))
    )
}

fn pretty_yaml(body: &str) -> Option<String> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(body).ok()?;
    serde_yaml::to_string(&value).ok()
}

fn code_panel(body: &str, mono: &str, theme: &Theme) -> AnyElement {
    let lines = body.split('\n').map(|line| {
        let leading = line
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .count();
        let indent = "\u{00a0}".repeat(leading);
        let content = line.trim_start_matches([' ', '\t']);
        div()
            .text_xs()
            .whitespace_nowrap()
            .font_family(mono.to_string())
            .child(format!("{indent}{content}"))
    });
    div()
        .min_h_0()
        .flex_1()
        .id("response-body")
        .overflow_y_scroll()
        .rounded_md()
        .bg(theme.muted)
        .p_3()
        .child(v_flex().children(lines))
        .into_any_element()
}

/// 单行键值：左侧名称（定宽、弱化），右侧值。响应头和 Cookie 共用。
fn kv_line(
    theme: &Theme,
    name: impl Into<SharedString>,
    value: impl Into<SharedString>,
) -> impl IntoElement {
    h_flex()
        .gap_2()
        .text_xs()
        .py_0p5()
        .child(
            div()
                .w(px(200.))
                .flex_shrink_0()
                .text_color(theme.muted_foreground)
                .text_ellipsis()
                .child(name.into()),
        )
        .child(div().flex_1().text_ellipsis().child(value.into()))
}

fn headers_view(headers: &std::collections::BTreeMap<String, Vec<String>>, cx: &App) -> AnyElement {
    let theme = cx.theme().clone();
    if headers.is_empty() {
        return div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(tr!(cx, "响应没有头。", "No response headers."))
            .into_any_element();
    }
    // 复制成 `k: v` 行，方便贴进 issue 或日志。
    let copy_text: String = headers
        .iter()
        .flat_map(|(name, values)| values.iter().map(move |value| format!("{name}: {value}")))
        .collect::<Vec<_>>()
        .join("\n");
    v_flex()
        .id("response-headers")
        .overflow_y_scroll()
        .flex_1()
        .gap_1()
        .child(
            h_flex().justify_end().child(
                Clipboard::new("copy-response-headers")
                    .small()
                    .value(copy_text)
                    .tooltip(tr!(cx, "复制响应头", "Copy headers")),
            ),
        )
        .children(
            headers
                .iter()
                .flat_map(|(name, values)| values.iter().map(move |value| (name, value)))
                .map(|(name, value)| kv_line(&theme, name.as_str(), value.as_str())),
        )
        .into_any_element()
}

/// 从多个 `set-cookie` 头解析 cookie 列表。
fn parse_cookies(
    headers: &std::collections::BTreeMap<String, Vec<String>>,
) -> Vec<(String, String)> {
    let mut cookies = Vec::new();
    for (name, values) in headers {
        if !name.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        for value in values {
            let Some(pair) = value.split(';').next() else {
                continue;
            };
            let mut split = pair.splitn(2, '=');
            let key = split.next().unwrap_or("").trim().to_string();
            let item = split.next().unwrap_or("").trim().to_string();
            if !key.is_empty() {
                cookies.push((key, item));
            }
        }
    }
    cookies
}

fn cookies_view(headers: &std::collections::BTreeMap<String, Vec<String>>, cx: &App) -> AnyElement {
    let theme = cx.theme().clone();
    let cookies = parse_cookies(headers);
    if cookies.is_empty() {
        return div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(tr!(cx, "响应没有设置 Cookie。", "No cookies were set."))
            .into_any_element();
    }
    v_flex()
        .id("response-cookies")
        .overflow_y_scroll()
        .flex_1()
        .gap_1()
        .children(
            cookies
                .into_iter()
                .map(|(key, value)| kv_line(&theme, key, value)),
        )
        .into_any_element()
}

fn console_view(response: &thread_core::services::http::HttpResponse, cx: &App) -> AnyElement {
    let timing = &response.timing;
    let mut lines = vec![
        format!("{}  {}", response.status_code, response.url),
        format!(
            "{}  ·  {}",
            format_bytes(response.body_size),
            response
                .content_type
                .clone()
                .unwrap_or_else(|| tr!(cx, "未知类型", "unknown type").to_string())
        ),
        format!(
            "dns {}   tcp {}   tls {}   ttfb {}   download {}   total {}",
            format_duration(timing.dns_ms),
            format_duration(timing.tcp_ms),
            format_duration(timing.tls_ms),
            format_duration(timing.ttfb_ms),
            format_duration(timing.download_ms),
            format_duration(timing.total_ms),
        ),
        tr!(
            cx,
            format!("重定向 {} 次", response.redirect_count),
            format!("{} redirects", response.redirect_count)
        ),
    ];
    if response.unresolved_variables.is_empty() {
        lines.push(tr!(cx, "变量都已替换。", "Every variable was replaced.").to_string());
    } else {
        lines.push(tr!(
            cx,
            format!("未定义的变量：{}", response.unresolved_variables.join("、")),
            format!(
                "Undefined variables: {}",
                response.unresolved_variables.join(", ")
            )
        ));
    }
    if timing.tls_ms == 0.0 {
        lines.push(
            tr!(
                cx,
                "TLS 耗时包含在首字节里，没有单独计时。",
                "TLS time is included in the first byte, not measured on its own."
            )
            .to_string(),
        );
    }
    let mono = cx.theme().mono_font_family.clone();
    let muted = cx.theme().muted_foreground;
    v_flex()
        .id("response-console")
        .overflow_y_scroll()
        .flex_1()
        .gap_1()
        .children(lines.into_iter().map(|line| {
            div()
                .text_xs()
                .font_family(mono.clone())
                .text_color(muted)
                .child(line)
        }))
        .into_any_element()
}

fn response_section_label(
    section: ResponseSection,
    headers: &std::collections::BTreeMap<String, Vec<String>>,
    cx: &App,
) -> SharedString {
    let base = section.label(cx);
    let count = match section {
        ResponseSection::Headers => headers.values().map(Vec::len).sum::<usize>(),
        ResponseSection::Cookies => parse_cookies(headers).len(),
        ResponseSection::Body | ResponseSection::Timing | ResponseSection::Console => 0,
    };
    if count == 0 {
        SharedString::from(base)
    } else {
        SharedString::from(format!("{base} ({count})"))
    }
}

fn content_type_label(content_type: &str) -> String {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "application/json" | "text/json" => "JSON".to_string(),
        "application/xml" | "text/xml" => "XML".to_string(),
        "text/html" => "HTML".to_string(),
        "text/plain" => "Text".to_string(),
        "application/yaml" | "text/yaml" | "application/x-yaml" => "YAML".to_string(),
        "application/octet-stream" => "Binary".to_string(),
        other => other
            .rsplit('/')
            .next()
            .unwrap_or(other)
            .to_ascii_uppercase(),
    }
}

fn timing_view(timing: &thread_core::services::http::Timing, cx: &App) -> AnyElement {
    let theme = cx.theme().clone();
    let total = timing.total_ms.max(0.0);
    let rows = [
        ("DNS", timing.dns_ms, false),
        ("TCP", timing.tcp_ms, false),
        ("TLS", timing.tls_ms, false),
        (tr!(cx, "首字节", "First byte"), timing.ttfb_ms, false),
        (tr!(cx, "下载", "Download"), timing.download_ms, false),
        (tr!(cx, "总计", "Total"), timing.total_ms, true),
    ];
    v_flex()
        .id("response-timing")
        .overflow_y_scroll()
        .flex_1()
        .gap_2()
        .children(rows.into_iter().map(|(label, ms, total_row)| {
            let ratio = if total > 0.0 {
                (ms / total).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            v_flex()
                .gap_1()
                .child(widgets::field_row(cx, label, format_duration(ms)))
                .child(
                    div()
                        .relative()
                        .h(px(4.))
                        .w_full()
                        .rounded_full()
                        .bg(theme.muted)
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .h(px(4.))
                                .rounded_full()
                                .bg(if total_row {
                                    theme.foreground
                                } else {
                                    theme.primary
                                })
                                .w(gpui_kit::relative(ratio)),
                        ),
                )
        }))
        .into_any_element()
}

#[cfg(test)]
mod body_format_tests {
    use super::{ResponseBodyMode, detect_format, render_body};

    #[test]
    fn formats_json_from_content_type() {
        let raw = r#"{"ok":true,"count":2}"#;
        assert_eq!(
            detect_format(raw, Some("application/json")),
            ResponseBodyMode::Json
        );
        let (text, parse_error) = render_body(raw, ResponseBodyMode::Json);
        assert!(text.contains("\n  \"ok\": true"));
        assert!(!parse_error);
    }

    #[test]
    fn formats_xml_from_content_type() {
        let raw = "<root><item>value</item></root>";
        assert_eq!(
            detect_format(raw, Some("application/xml")),
            ResponseBodyMode::Xml
        );
        let (text, parse_error) = render_body(raw, ResponseBodyMode::Xml);
        assert!(text.contains("\n  <item>"));
        assert!(!parse_error);
    }

    #[test]
    fn formats_yaml_from_content_type() {
        let raw = "name: Thread\nitems:\n- one\n- two";
        assert_eq!(
            detect_format(raw, Some("application/yaml")),
            ResponseBodyMode::Yaml
        );
        let (text, parse_error) = render_body(raw, ResponseBodyMode::Yaml);
        assert!(text.contains("name: Thread"));
        assert!(text.contains("- one"));
        assert!(!parse_error);
    }

    #[test]
    fn detects_yaml_with_plain_text_content_type() {
        let raw = "name: Thread\nitems:\n- one";
        assert_eq!(
            detect_format(raw, Some("text/plain")),
            ResponseBodyMode::Yaml
        );
        let (_, parse_error) = render_body(raw, ResponseBodyMode::Yaml);
        assert!(!parse_error);
    }

    #[test]
    fn html_stays_source() {
        let raw = "<h1>Hi &amp; welcome</h1><script>alert('x')</script><p>Safe text</p>";
        assert_eq!(
            detect_format(raw, Some("text/html")),
            ResponseBodyMode::Html
        );
        let (text, parse_error) = render_body(raw, ResponseBodyMode::Html);
        assert_eq!(text, raw);
        assert!(!parse_error);
    }
}
