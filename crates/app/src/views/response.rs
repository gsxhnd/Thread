//! 响应面板：状态行 + 响应体/响应头/Cookie/耗时。

use gpui_kit::assets::IconName;
use gpui_kit::component::clipboard::Clipboard;
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

/// 响应体的展示方式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ResponseBodyMode {
    #[default]
    Pretty,
    Raw,
    Preview,
}

impl ResponseBodyMode {
    const ALL: [Self; 3] = [Self::Pretty, Self::Raw, Self::Preview];

    fn label(self) -> &'static str {
        match self {
            Self::Pretty => "Pretty",
            Self::Raw => "Raw",
            Self::Preview => "Preview",
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
                .child(candidate.label(cx))
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

    let formatted = format_response_body(body, content_type);
    let display = match mode {
        ResponseBodyMode::Pretty => formatted.text.clone(),
        ResponseBodyMode::Raw => body.to_string(),
        ResponseBodyMode::Preview => preview_body(body, content_type, cx),
    };
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
                .flex_wrap()
                .child(
                    h_flex()
                        .gap_1()
                        .children(ResponseBodyMode::ALL.map(|candidate| {
                            let selected = mode == candidate;
                            div()
                                .id(SharedString::from(format!(
                                    "response-body-mode-{candidate:?}"
                                )))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_xs()
                                .cursor_pointer()
                                .when(selected, |this| {
                                    this.bg(theme.accent)
                                        .text_color(theme.foreground)
                                        .font_weight(FontWeight::MEDIUM)
                                })
                                .when(!selected, |this| {
                                    this.text_color(theme.muted_foreground).hover(|this| {
                                        this.bg(theme.muted).text_color(theme.foreground)
                                    })
                                })
                                .child(candidate.label())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(tab) = this.tabs.get_mut(tab_ix) {
                                        tab.response_body_mode = candidate;
                                        cx.notify();
                                    }
                                }))
                        })),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(widgets::mini_tag(cx, formatted.format))
                        .when(
                            mode == ResponseBodyMode::Pretty && formatted.parse_error,
                            |row| {
                                row.child(div().text_xs().text_color(theme.muted_foreground).child(
                                    tr!(
                                        cx,
                                        "格式解析失败，显示原始文本",
                                        "Could not parse; showing raw text"
                                    ),
                                ))
                            },
                        )
                        .child(
                            Clipboard::new("copy-response-body")
                                .small()
                                .value(display.clone())
                                .tooltip(tr!(cx, "复制当前视图", "Copy current view")),
                        ),
                ),
        )
        .child(if mode == ResponseBodyMode::Preview {
            preview_panel(body, content_type, &theme, cx)
        } else {
            code_panel(&display, &mono, &theme)
        })
        .into_any_element()
}

struct FormattedBody {
    text: String,
    format: &'static str,
    parse_error: bool,
}

fn format_response_body(body: &str, content_type: Option<&str>) -> FormattedBody {
    let mime = content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let detected = if mime.contains("json")
        || (mime.is_empty()
            && (body.trim_start().starts_with('{') || body.trim_start().starts_with('[')))
    {
        Some(("JSON", pretty_json as fn(&str) -> Option<String>))
    } else if mime.contains("yaml")
        || mime.contains("yml")
        || ((mime.is_empty() || mime == "text/plain") && looks_like_yaml(body))
    {
        Some(("YAML", pretty_yaml as fn(&str) -> Option<String>))
    } else if mime.contains("html") {
        return FormattedBody {
            text: body.to_string(),
            format: "HTML",
            parse_error: false,
        };
    } else if mime.contains("xml")
        || (mime.is_empty()
            && (body.trim_start().starts_with("<?xml") || body.trim_start().starts_with('<')))
    {
        Some(("XML", pretty_xml as fn(&str) -> Option<String>))
    } else {
        None
    };

    let Some((format, parser)) = detected else {
        return FormattedBody {
            text: body.to_string(),
            format: "TEXT",
            parse_error: false,
        };
    };
    match parser(body) {
        Some(text) => FormattedBody {
            text,
            format,
            parse_error: false,
        },
        None => FormattedBody {
            text: body.to_string(),
            format,
            parse_error: true,
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

fn is_html(content_type: Option<&str>, body: &str) -> bool {
    let mime = content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    mime.contains("html")
        || body
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype html")
        || body.trim_start().to_ascii_lowercase().starts_with("<html")
}

/// Preview renders HTML as safe text; scripts, styles, and embedded resources are never executed.
fn preview_body(body: &str, content_type: Option<&str>, cx: &App) -> String {
    if is_html(content_type, body) {
        html_to_text(body)
    } else if content_type.is_some_and(|mime| {
        let mime = mime.to_ascii_lowercase();
        mime.starts_with("image/") || mime.contains("pdf")
    }) {
        tr!(
            cx,
            "此响应类型暂不支持内嵌预览，请切换到 Raw 查看内容。",
            "Inline preview is unavailable for this response type. Switch to Raw to inspect it."
        )
        .to_string()
    } else {
        body.to_string()
    }
}

fn html_to_text(html: &str) -> String {
    let mut output = String::new();
    let mut rest = html;
    let mut hidden_tag: Option<String> = None;
    while !rest.is_empty() {
        if let Some(tag_start) = rest.find('<') {
            if tag_start > 0 && hidden_tag.is_none() {
                output.push_str(&decode_html_entities(&rest[..tag_start]));
            }
            rest = &rest[tag_start..];
            let Some(tag_end) = rest.find('>') else {
                break;
            };
            let raw_tag = rest[1..tag_end].trim();
            let closing = raw_tag.starts_with('/');
            let name = raw_tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(
                name.as_str(),
                "script" | "style" | "iframe" | "object" | "svg"
            ) {
                if closing && hidden_tag.as_deref() == Some(name.as_str()) {
                    hidden_tag = None;
                } else if !closing {
                    hidden_tag = Some(name);
                }
            } else if hidden_tag.is_none()
                && matches!(
                    name.as_str(),
                    "br" | "hr"
                        | "li"
                        | "p"
                        | "div"
                        | "section"
                        | "article"
                        | "ul"
                        | "ol"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "tr"
                )
            {
                if name == "li" && !closing {
                    output.push_str("\n• ");
                } else {
                    output.push('\n');
                }
            }
            rest = &rest[tag_end + 1..];
        } else {
            if hidden_tag.is_none() {
                output.push_str(&decode_html_entities(rest));
            }
            break;
        }
    }
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn decode_html_entities(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let entity_start = start + 1;
        let Some(end) = rest[entity_start..].find(';') else {
            decoded.push_str(&rest[start..]);
            return decoded;
        };
        let entity = &rest[entity_start..entity_start + end];
        let value = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(ch) = value {
            decoded.push(ch);
        } else {
            decoded.push_str(&rest[start..entity_start + end + 1]);
        }
        rest = &rest[entity_start + end + 1..];
    }
    decoded.push_str(rest);
    decoded
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

fn preview_panel(body: &str, content_type: Option<&str>, theme: &Theme, cx: &App) -> AnyElement {
    let preview = preview_body(body, content_type, cx);
    let is_html_response = is_html(content_type, body);
    div()
        .min_h_0()
        .flex_1()
        .id("response-preview")
        .overflow_y_scroll()
        .rounded_md()
        .bg(theme.background)
        .p_4()
        .child(
            v_flex()
                .gap_2()
                .when(is_html_response, |view| {
                    view.child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(tr!(
                                cx,
                                "安全静态预览：脚本、样式和外部资源不会执行。",
                                "Safe static preview: scripts, styles, and external resources are not run."
                            )),
                    )
                })
                .child(v_flex().children(preview.lines().map(|line| {
                    div().text_sm().py_0p5().child(line.to_string())
                }))),
        )
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

fn timing_view(timing: &thread_core::services::http::Timing, cx: &App) -> AnyElement {
    let rows = [
        ("DNS", timing.dns_ms),
        ("TCP", timing.tcp_ms),
        ("TLS", timing.tls_ms),
        (tr!(cx, "首字节", "First byte"), timing.ttfb_ms),
        (tr!(cx, "下载", "Download"), timing.download_ms),
        (tr!(cx, "总计", "Total"), timing.total_ms),
    ];
    v_flex()
        .id("response-timing")
        .overflow_y_scroll()
        .flex_1()
        .gap_1()
        .children(
            rows.into_iter()
                .map(|(label, ms)| widgets::field_row(cx, label, format_duration(ms))),
        )
        .into_any_element()
}

#[cfg(test)]
mod body_format_tests {
    use super::{format_response_body, html_to_text};

    #[test]
    fn pretty_formats_json_from_content_type() {
        let body = format_response_body(r#"{"ok":true,"count":2}"#, Some("application/json"));
        assert_eq!(body.format, "JSON");
        assert!(body.text.contains("\n  \"ok\": true"));
        assert!(!body.parse_error);
    }

    #[test]
    fn pretty_formats_xml_from_content_type() {
        let body = format_response_body("<root><item>value</item></root>", Some("application/xml"));
        assert_eq!(body.format, "XML");
        assert!(body.text.contains("\n  <item>"));
        assert!(!body.parse_error);
    }

    #[test]
    fn pretty_formats_yaml_from_content_type() {
        let body = format_response_body(
            "name: Thread\nitems:\n- one\n- two",
            Some("application/yaml"),
        );
        assert_eq!(body.format, "YAML");
        assert!(body.text.contains("name: Thread"));
        assert!(body.text.contains("- one"));
        assert!(!body.parse_error);
    }

    #[test]
    fn pretty_detects_yaml_with_plain_text_content_type() {
        let body = format_response_body("name: Thread\nitems:\n- one", Some("text/plain"));
        assert_eq!(body.format, "YAML");
        assert!(!body.parse_error);
    }

    #[test]
    fn html_preview_strips_active_content_and_decodes_entities() {
        let text =
            html_to_text("<h1>Hi &amp; welcome</h1><script>alert('x')</script><p>Safe text</p>");
        assert!(text.contains("Hi & welcome"));
        assert!(text.contains("Safe text"));
        assert!(!text.contains("alert"));
    }
}
