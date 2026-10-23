//! 设置子窗口：通用、外观主题、快捷键与关于页面。

use std::sync::Arc;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, TitleBar, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, FontWeight, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, WeakEntity, Window,
    WindowBounds, div, point, px, size,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::AppCore;
use thread_core::config::AppSettings;

use crate::app::AppView;
use crate::theme::ThemePreference;
use crate::views::shortcuts;
use crate::widgets;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    General,
    Appearance,
    Shortcuts,
    About,
}

pub struct SettingsWindow {
    core: Arc<AppCore>,
    app: WeakEntity<AppView>,
    page: SettingsPage,
    timeout: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsWindow {
    pub fn new(
        core: Arc<AppCore>,
        app: WeakEntity<AppView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let timeout = cx.new(|cx| {
            let millis = core.config.app_settings().default_timeout_ms;
            InputState::new(window, cx).default_value(millis.to_string())
        });

        let mut subscriptions = Vec::new();
        let timeout_input = timeout.clone();
        subscriptions.push(cx.subscribe_in(
            &timeout_input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    return;
                }
                let Ok(timeout) = this.timeout.read(cx).value().trim().parse::<u64>() else {
                    return;
                };
                this.persist(cx, |settings| settings.default_timeout_ms = timeout);
            },
        ));
        if let Some(app_entity) = app.upgrade() {
            subscriptions.push(cx.observe(&app_entity, |_, _, cx| cx.notify()));
        }

        Self {
            core,
            app,
            page: SettingsPage::General,
            timeout,
            _subscriptions: subscriptions,
        }
    }

    fn persist(&mut self, cx: &mut Context<Self>, mutate: impl FnOnce(&mut AppSettings)) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        app.update(cx, |view, cx| view.update_app_settings(cx, mutate));
        cx.notify();
    }

    fn theme_preference(&self, cx: &App) -> ThemePreference {
        self.app
            .read_with(cx, |view, _| view.theme)
            .unwrap_or_default()
    }

    fn render_general(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.core.config.app_settings();
        let paths = self.core.paths();
        let locale = crate::i18n::current(cx);

        v_flex()
            .gap_5()
            .px_6()
            .py_5()
            .child(widgets::page_header(
                cx,
                tr!(cx, "通用", "General"),
                tr!(
                    cx,
                    "语言与 HTTP 默认值保存在本机配置里；代理端口和当前环境属于工作区。",
                    "Language and HTTP defaults are stored on this machine. The proxy port and active environment belong to the workspace."
                ),
                Vec::<AnyElement>::new(),
            ))
            .child(widgets::section(
                cx,
                tr!(cx, "语言", "Language"),
                tr!(
                    cx,
                    "界面文字。选择会记住，下次启动恢复。",
                    "Interface language. The choice is remembered."
                ),
                {
                    let app = self.app.clone();
                    let settings = cx.entity().downgrade();
                    Button::new("locale-menu")
                        .outline()
                        .small()
                        .label(locale.label())
                        .dropdown_menu(move |menu, _, _| {
                            let mut menu = menu;
                            for choice in crate::i18n::Locale::ALL {
                                let label = if locale == choice {
                                    format!("✓ {}", choice.label())
                                } else {
                                    choice.label().to_string()
                                };
                                let app = app.clone();
                                let settings = settings.clone();
                                menu = menu.item(PopupMenuItem::new(label).on_click(
                                    move |_, _, cx| {
                                        let _ = app.update_in(cx, |view, window, cx| {
                                            view.set_locale(choice, window, cx);
                                        });
                                        let _ = settings.update(cx, |_, cx| cx.notify());
                                    },
                                ));
                            }
                            menu
                        })
                },
            ))
            .child(widgets::section(
                cx,
                tr!(cx, "HTTP 默认值", "HTTP defaults"),
                tr!(
                    cx,
                    "单个请求或集合没有自己的设置时，用这里的值。",
                    "Used when a request or collection does not set its own."
                ),
                v_flex()
                    .child(setting_switch_row(
                        cx,
                        "follow-redirects",
                        tr!(cx, "跟随重定向", "Follow redirects"),
                        tr!(
                            cx,
                            "发送后自动跟随 3xx，最多 10 次。",
                            "Follow 3xx responses, up to 10 hops."
                        ),
                        settings.follow_redirects,
                        cx.listener(|this, checked: &bool, _, cx| {
                            let checked = *checked;
                            this.persist(cx, |settings| settings.follow_redirects = checked);
                        }),
                    ))
                    .child(setting_switch_row(
                        cx,
                        "verify-ssl",
                        tr!(cx, "校验 TLS 证书", "Verify TLS certificates"),
                        tr!(
                            cx,
                            "关闭后忽略证书错误，只建议在本地调试时使用。",
                            "When off, certificate errors are ignored. Only for local debugging."
                        ),
                        settings.verify_ssl,
                        cx.listener(|this, checked: &bool, _, cx| {
                            let checked = *checked;
                            this.persist(cx, |settings| settings.verify_ssl = checked);
                        }),
                    ))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .py_3()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(div().text_sm().child(tr!(cx, "默认超时", "Default timeout")))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(tr!(
                                                cx,
                                                "单位是毫秒。0 表示一直等到返回。",
                                                "Milliseconds. 0 waits until the server responds."
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(120.))
                                    .flex_shrink_0()
                                    .child(Input::new(&self.timeout).small()),
                            ),
                    ),
            ))
            .child(widgets::section(
                cx,
                tr!(cx, "本地数据", "Local data"),
                tr!(
                    cx,
                    "历史、代理会话和证书在下面这些目录。工作区目录只保存集合、请求和环境。",
                    "History, proxy sessions, and certificates live in these folders. The workspace only stores collections, requests, and environments."
                ),
                v_flex()
                    .gap_3()
                    .child(path_row(cx, tr!(cx, "配置文件", "Config file"), paths.config_file))
                    .child(path_row(cx, tr!(cx, "数据目录", "Data folder"), paths.data_dir))
                    .child(path_row(cx, tr!(cx, "数据库", "Database"), paths.db_file))
                    .child(path_row(cx, tr!(cx, "日志目录", "Log folder"), paths.log_dir)),
            ))
            .into_any_element()
    }

    fn render_appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme_pref = self.theme_preference(cx);
        let layout = self
            .app
            .read_with(cx, |view, _| view.editor_layout)
            .unwrap_or_default();

        v_flex()
            .gap_5()
            .px_6()
            .py_5()
            .child(widgets::page_header(
                cx,
                tr!(cx, "外观", "Appearance"),
                tr!(
                    cx,
                    "为工作台选择主题，并调整请求与响应面板的默认布局。",
                    "Choose a workspace theme and set the default request/response layout."
                ),
                Vec::<AnyElement>::new(),
            ))
            .child(widgets::section(
                cx,
                tr!(cx, "主题", "Theme"),
                tr!(
                    cx,
                    "选择会立即应用，并保存在本机配置中。",
                    "Changes apply immediately and are saved to this machine."
                ),
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_2()
                    .children(ThemePreference::ALL.map(|preference| {
                        let selected = theme_pref == preference;
                        let (background, foreground, accent) = preference.swatch();
                        v_flex()
                            .id(SharedString::from(format!(
                                "theme-pref-{}",
                                preference.code()
                            )))
                            .w(px(150.))
                            .gap_2()
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .cursor_pointer()
                            .when(selected, |this| {
                                this.border_color(cx.theme().primary).bg(cx.theme().accent)
                            })
                            .when(!selected, |this| {
                                this.border_color(cx.theme().border)
                                    .hover(|this| this.bg(cx.theme().muted))
                            })
                            .child(
                                h_flex()
                                    .h(px(34.))
                                    .w_full()
                                    .rounded_sm()
                                    .overflow_hidden()
                                    .bg(gpui_kit::rgb(background))
                                    .border_1()
                                    .border_color(gpui_kit::rgb(foreground))
                                    .child(div().w(px(12.)).h_full().bg(gpui_kit::rgb(accent)))
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .justify_center()
                                            .gap_1()
                                            .px_2()
                                            .child(
                                                div()
                                                    .w(px(42.))
                                                    .h(px(4.))
                                                    .rounded_sm()
                                                    .bg(gpui_kit::rgb(foreground)),
                                            )
                                            .child(
                                                div()
                                                    .w(px(26.))
                                                    .h(px(3.))
                                                    .rounded_sm()
                                                    .bg(gpui_kit::rgb(foreground))
                                                    .opacity(0.55),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .when(selected, |this| this.font_weight(FontWeight::MEDIUM))
                                    .child(preference.label(cx)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(app) = this.app.upgrade() {
                                    app.update(cx, |view, cx| {
                                        view.set_theme(preference, window, cx)
                                    });
                                }
                                cx.notify();
                            }))
                    })),
            ))
            .child(widgets::section(
                cx,
                tr!(cx, "请求 / 响应布局", "Request / response layout"),
                tr!(
                    cx,
                    "控制编辑器与响应面板的默认排列方式。",
                    "Choose how the editor and response panel are arranged by default."
                ),
                h_flex().gap_2().children(
                    [
                        (
                            crate::theme::EditorLayout::Stacked,
                            tr!(cx, "上下排列", "Stacked"),
                        ),
                        (
                            crate::theme::EditorLayout::Columns,
                            tr!(cx, "左右并排", "Side by side"),
                        ),
                    ]
                    .map(|(choice, label)| {
                        let selected = layout == choice;
                        h_flex()
                            .id(SharedString::from(format!(
                                "editor-layout-{}",
                                match choice {
                                    crate::theme::EditorLayout::Stacked => "stacked",
                                    crate::theme::EditorLayout::Columns => "columns",
                                }
                            )))
                            .flex_1()
                            .h(px(42.))
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .border_1()
                            .text_sm()
                            .cursor_pointer()
                            .when(selected, |this| {
                                this.border_color(cx.theme().primary).bg(cx.theme().accent)
                            })
                            .when(!selected, |this| {
                                this.border_color(cx.theme().border)
                                    .hover(|this| this.bg(cx.theme().muted))
                            })
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(app) = this.app.upgrade() {
                                    app.update(cx, |view, cx| view.set_editor_layout(choice, cx));
                                }
                                cx.notify();
                            }))
                    }),
                ),
            ))
            .into_any_element()
    }

    fn render_about(&self, cx: &mut Context<Self>) -> AnyElement {
        let paths = self.core.paths();
        let version = env!("CARGO_PKG_VERSION");
        v_flex()
            .gap_5()
            .px_6()
            .py_5()
            .child(widgets::page_header(
                cx,
                tr!(cx, "关于 Thread", "About Thread"),
                tr!(cx, "应用信息、支持的协议与本地数据说明。", "App information, supported protocols, and local data details."),
                Vec::<AnyElement>::new(),
            ))
            .child(
                h_flex()
                    .items_center()
                    .gap_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary)
                    .p_5()
                    .child(
                        div()
                            .size_12()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .bg(cx.theme().primary)
                            .text_color(cx.theme().primary_foreground)
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .child("T"),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Thread / 灰线"))
                            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(tr!(cx, "本地优先的 API 开发与调试工作台", "A local-first API development and debugging workbench")))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{} · {}", tr!(cx, "版本", "Version"), version))),
                    ),
            )
            .child(widgets::section(
                cx,
                tr!(cx, "为 API 工作流而建", "Built for API workflows"),
                tr!(cx, "集合与请求以文件形式保存在工作区；运行记录与代理数据留在本机。", "Collections and requests are files in your workspace; run history and proxy data stay local."),
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .children(["HTTP", "GraphQL", "WebSocket", "gRPC", "MQTT", "Proxy"].map(|label| {
                        div()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(cx.theme().muted)
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .child(label)
                    })),
            ))
            .child(widgets::section(
                cx,
                tr!(cx, "本机存储位置", "Local storage"),
                tr!(cx, "工作区可使用 Git 管理；以下路径仅供本机应用数据参考。", "Workspaces can be managed with Git. These paths contain local app data."),
                v_flex()
                    .gap_3()
                    .child(path_row(cx, tr!(cx, "配置文件", "Config file"), paths.config_file))
                    .child(path_row(cx, tr!(cx, "数据目录", "Data folder"), paths.data_dir)),
            ))
            .into_any_element()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let page = self.page;
        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(TitleBar::new().child(div().text_sm().child(tr!(cx, "设置", "Settings"))))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(
                        v_flex()
                            .w(px(168.))
                            .flex_shrink_0()
                            .h_full()
                            .gap_1()
                            .px_3()
                            .py_4()
                            .border_r_1()
                            .border_color(theme.border)
                            .bg(theme.sidebar)
                            .child(page_tab(
                                cx,
                                "settings-page-general",
                                tr!(cx, "通用", "General"),
                                page == SettingsPage::General,
                                SettingsPage::General,
                            ))
                            .child(page_tab(
                                cx,
                                "settings-page-appearance",
                                tr!(cx, "外观与主题", "Appearance & theme"),
                                page == SettingsPage::Appearance,
                                SettingsPage::Appearance,
                            ))
                            .child(page_tab(
                                cx,
                                "settings-page-shortcuts",
                                tr!(cx, "快捷键", "Shortcuts"),
                                page == SettingsPage::Shortcuts,
                                SettingsPage::Shortcuts,
                            ))
                            .child(page_tab(
                                cx,
                                "settings-page-about",
                                tr!(cx, "关于", "About"),
                                page == SettingsPage::About,
                                SettingsPage::About,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .id("settings-page")
                            .overflow_y_scroll()
                            .child(match page {
                                SettingsPage::General => self.render_general(cx),
                                SettingsPage::Appearance => self.render_appearance(cx),
                                SettingsPage::Shortcuts => {
                                    shortcuts::render(window, cx).into_any_element()
                                }
                                SettingsPage::About => self.render_about(cx),
                            }),
                    ),
            )
    }
}

fn page_tab(
    cx: &mut Context<SettingsWindow>,
    id: &'static str,
    label: &'static str,
    selected: bool,
    page: SettingsPage,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    div()
        .id(id)
        .w_full()
        .px_3()
        .py_2()
        .rounded_md()
        .text_sm()
        .cursor_pointer()
        .when(selected, |this| {
            this.bg(theme.accent)
                .text_color(theme.foreground)
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!selected, |this| {
            this.hover(|this| this.bg(theme.muted))
                .text_color(theme.muted_foreground)
        })
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.page = page;
            cx.notify();
        }))
}

fn setting_switch_row(
    cx: &App,
    id: &'static str,
    label: &str,
    description: &str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = cx.theme();
    h_flex()
        .id(SharedString::from(format!("app-setting-{id}")))
        .items_center()
        .justify_between()
        .gap_4()
        .py_3()
        .border_b_1()
        .border_color(theme.border)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().child(label.to_string()))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .whitespace_normal()
                        .child(description.to_string()),
                ),
        )
        .child(Switch::new(id).checked(checked).on_click(on_click))
}

fn path_row(cx: &App, label: &str, value: impl Into<SharedString>) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .gap_1()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .w_full()
                .min_w_0()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .whitespace_normal()
                .child(value.into()),
        )
}

impl AppView {
    /// 打开设置子窗口；已经打开时只聚焦，不另开一个。
    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_pending {
            return;
        }
        if let Some(handle) = self.settings_window
            && cx
                .update_window(handle, |_, child, _| child.activate_window())
                .is_ok()
        {
            return;
        }

        self.settings_pending = true;
        let core = self.core.clone();
        let app = cx.entity().downgrade();
        let bounds = window.bounds();
        window.defer(cx, move |_, cx| {
            let width = px(760.);
            let height = px(560.);
            let origin = point(
                bounds.origin.x + (bounds.size.width - width) / 2.,
                bounds.origin.y + (bounds.size.height - height) / 2.,
            );
            let mut options = TitleBar::window_options();
            options.window_bounds = Some(WindowBounds::Windowed(Bounds::new(
                origin,
                size(width, height),
            )));
            options.window_min_size = Some(size(px(640.), px(420.)));
            options.focus = true;
            options.app_id = Some("thread-gpui".into());
            if let Some(titlebar) = options.titlebar.as_mut() {
                titlebar.title = Some(tr!(cx, "设置", "Settings").into());
            }
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| SettingsWindow::new(core, app.clone(), window, cx))
            });
            let handle = match opened {
                Ok((handle, _)) => Some(handle),
                Err(error) => {
                    tracing::error!(%error, "failed to open settings window");
                    None
                }
            };
            let _ = app.update(cx, |view, cx| {
                view.settings_pending = false;
                if let Some(handle) = handle {
                    view.settings_window = Some(handle);
                    cx.notify();
                }
            });
        });
    }

    pub fn update_app_settings(
        &mut self,
        cx: &mut Context<Self>,
        mutate: impl FnOnce(&mut AppSettings),
    ) {
        let mut settings = self.core.config.app_settings();
        mutate(&mut settings);
        if let Err(error) = self.core.config.set_app_settings(&settings) {
            tracing::warn!(?error, "failed to save app settings");
            return;
        }
        cx.notify();
    }
}
