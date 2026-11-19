//! 环境视图：环境列表 + 变量编辑 + 激活。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _,
    PromptLevel, SharedString, Styled as _, Subscription, Task, WeakEntity, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use std::time::Duration;
use thread_core::error::AppError;
use thread_core::workspace::index::EnvironmentSummary;

use crate::app::AppView;
use crate::widgets;

/// 值输入停下来的这段毫秒数之后自动保存。继续输入会重置计时。
const ENV_AUTOSAVE_DELAY: Duration = Duration::from_millis(600);

/// 一行环境变量的名称和值。订阅写在输入框前面，丢掉这一行时先取消订阅，
/// 避免输入框销毁时的失焦再提交一次。
pub struct EnvVariableField {
    key: String,
    value: String,
    secret: bool,
    stored: bool,
    /// 持有输入框订阅。字段本身不读，丢掉这一行时订阅一起取消。
    #[expect(dead_code)]
    name_watch: Subscription,
    #[expect(dead_code)]
    value_watch: Subscription,
    name_input: Entity<InputState>,
    value_input: Entity<InputState>,
    /// 值输入的防抖自动保存任务。新的改动会替换（即取消）旧任务；
    /// 这一行的其它字段被丢弃时任务一起取消，不会把已删除的变量写回去。
    pending_save: Option<Task<()>>,
}

/// 侧边栏里的环境列表。变量编辑留在主区域。
pub fn sidebar(this: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let environments: Vec<EnvironmentSummary> = this
        .workspace
        .read(cx)
        .index()
        .map(|index| index.environments.clone())
        .unwrap_or_default();
    if environments.is_empty() {
        let weak = cx.entity().downgrade();
        return div()
            .id("env-empty")
            .flex_1()
            .pt_8()
            .context_menu(move |menu, _, cx| menu.item(new_environment_item(weak.clone(), cx)))
            .child(widgets::compact_empty(
                cx,
                IconName::Braces,
                tr!(
                    cx,
                    "还没有环境。右键可以新建。",
                    "No environments yet. Right-click to create one."
                ),
            ))
            .into_any_element();
    }

    let selected_path = this.environment.selected_path.clone();
    div()
        .flex_1()
        .min_h_0()
        .id("env-list")
        .overflow_y_scroll()
        .px_1()
        .py_1()
        .child(
            v_flex()
                .gap_0p5()
                .children(environments.into_iter().enumerate().map(|(ix, summary)| {
                    let selected = selected_path.as_deref() == Some(summary.path.as_str());
                    let menu_path = summary.path.clone();
                    let menu_name = summary.name.clone();
                    let menu_active = summary.is_active;
                    let menu_global = summary.is_global;
                    let menu_weak = cx.entity().downgrade();
                    v_flex()
                        .id(SharedString::from(summary.path.clone()))
                        .gap_0p5()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .when(selected, |this| this.bg(theme.accent))
                        .when(!selected, |this| this.hover(|this| this.bg(theme.muted)))
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_ellipsis()
                                        .when(summary.is_active, |this| {
                                            this.font_weight(FontWeight::SEMIBOLD)
                                        })
                                        .child(summary.name.clone()),
                                )
                                .when(summary.is_active, |this| {
                                    this.child(widgets::mini_tag(cx, tr!(cx, "使用中", "Active")))
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(tr!(
                                    cx,
                                    format!(
                                        "{} 个变量 · {} 个机密",
                                        summary.variable_count, summary.secret_count
                                    ),
                                    format!(
                                        "{} variables · {} secrets",
                                        summary.variable_count, summary.secret_count
                                    )
                                )),
                        )
                        .on_click(
                            cx.listener(move |this, _: &gpui_kit::ClickEvent, window, cx| {
                                this.select_environment(ix, window, cx);
                            }),
                        )
                        .context_menu(move |menu, _, cx| {
                            environment_menu(
                                menu,
                                menu_path.clone(),
                                menu_name.clone(),
                                menu_active,
                                menu_global,
                                menu_weak.clone(),
                                cx,
                            )
                        })
                        .into_any_element()
                })),
        )
        .into_any_element()
}

pub(crate) fn heading_menu(menu: PopupMenu, weak: WeakEntity<AppView>, cx: &App) -> PopupMenu {
    menu.item(new_environment_item(weak, cx))
}

fn new_environment_item(weak: WeakEntity<AppView>, cx: &App) -> PopupMenuItem {
    PopupMenuItem::new(tr!(cx, "新建环境", "New environment")).on_click(move |_, _window, cx| {
        let _ = weak.update_in(cx, |view, window, cx| view.create_environment(window, cx));
    })
}

fn environment_menu(
    menu: PopupMenu,
    path: String,
    name: String,
    active: bool,
    global: bool,
    weak: WeakEntity<AppView>,
    cx: &App,
) -> PopupMenu {
    let mut menu = if active {
        let weak = weak.clone();
        menu.item(PopupMenuItem::new(tr!(cx, "停用", "Deactivate")).on_click(
            move |_, _window, cx| {
                let _ = weak.update_in(cx, |view, window, cx| {
                    view.activate_environment(None, window, cx);
                });
            },
        ))
    } else {
        let weak = weak.clone();
        let path = path.clone();
        menu.item(
            PopupMenuItem::new(tr!(cx, "设为当前环境", "Set active")).on_click(
                move |_, _window, cx| {
                    let path = path.clone();
                    let _ = weak.update_in(cx, |view, window, cx| {
                        view.activate_environment(Some(path), window, cx);
                    });
                },
            ),
        )
    };
    let rename_path = path.clone();
    let rename_name = name.clone();
    let rename_weak = weak.clone();
    menu = menu
        .separator()
        .item(
            PopupMenuItem::new(tr!(cx, "重命名", "Rename")).on_click(move |_, window, cx| {
                prompt_environment_rename(
                    window,
                    cx,
                    rename_path.clone(),
                    rename_name.clone(),
                    rename_weak.clone(),
                );
            }),
        );
    if !global {
        let delete_path = path;
        let delete_name = name;
        let delete_weak = weak.clone();
        menu = menu
            .separator()
            .item(
                PopupMenuItem::new(tr!(cx, "删除", "Delete")).on_click(move |_, window, cx| {
                    confirm_delete_environment(
                        window,
                        cx,
                        delete_name.clone(),
                        delete_path.clone(),
                        delete_weak.clone(),
                    );
                }),
            );
    }
    menu.separator().item(new_environment_item(weak, cx))
}

fn prompt_environment_rename(
    window: &mut Window,
    cx: &mut App,
    path: String,
    current: String,
    weak: WeakEntity<AppView>,
) {
    let input = cx.new(|cx| InputState::new(window, cx).default_value(current));
    window.open_dialog(cx, move |dialog, _, cx| {
        let input = input.clone();
        let weak = weak.clone();
        let path = path.clone();
        dialog
            .title(tr!(cx, "重命名环境", "Rename environment"))
            .w(px(360.))
            .child(Input::new(&input))
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("env-rename-cancel")
                            .label(tr!(cx, "取消", "Cancel"))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("env-rename-ok")
                            .primary()
                            .label(tr!(cx, "确定", "OK"))
                            .on_click(move |_, window, cx| {
                                let name = input.read(cx).value().trim().to_string();
                                if name.is_empty() {
                                    window.push_notification(
                                        tr!(cx, "名称不能为空。", "Name cannot be empty."),
                                        cx,
                                    );
                                    return;
                                }
                                window.close_dialog(cx);
                                let path = path.clone();
                                let _ = weak.update_in(cx, |view, window, cx| {
                                    view.rename_environment(path, name, window, cx);
                                });
                            }),
                    ),
            )
    });
}

fn confirm_delete_environment(
    window: &mut Window,
    cx: &mut App,
    name: String,
    path: String,
    weak: WeakEntity<AppView>,
) {
    let message = tr!(
        cx,
        format!("删除环境「{name}」？"),
        format!("Delete environment “{name}”?")
    );
    let receiver = window.prompt(
        PromptLevel::Warning,
        &message,
        Some(tr!(
            cx,
            "文件会移到应用回收站，可以稍后找回。",
            "The file goes to the app trash and can be restored later."
        )),
        &[tr!(cx, "删除", "Delete"), tr!(cx, "取消", "Cancel")],
        cx,
    );
    cx.spawn(async move |cx| {
        if receiver.await == Ok(0) {
            let _ = weak.update_in(cx, |view, window, cx| {
                view.delete_environment(path, window, cx);
            });
        }
    })
    .detach();
}

pub fn render(this: &mut AppView, _window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let empty = this
        .workspace
        .read(cx)
        .index()
        .map(|index| index.environments.is_empty())
        .unwrap_or(true);
    if empty {
        return widgets::empty_state_with_action(
            cx,
            IconName::Braces,
            tr!(cx, "还没有环境", "No environments"),
            tr!(
                cx,
                "环境是一组 `{{变量}}`，写在 environments/*.env.json 里，随工作区进 Git；机密值只存本地。",
                "An environment is a set of `{{variables}}` in environments/*.env.json, committed with the workspace. Secrets stay on this machine."
            ),
            Button::new("new-environment")
                .primary()
                .label(tr!(cx, "新建环境", "New environment"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.create_environment(window, cx);
                })),
        )
        .into_any_element();
    }

    v_flex()
        .flex_1()
        .min_h_0()
        .p_3()
        .child(
            div()
                .flex_1()
                .min_h_0()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .p_3()
                .child(detail_view(this, cx)),
        )
        .into_any_element()
}

fn detail_view(this: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(detail) = this.environment.detail.clone() else {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(
                        cx,
                        "从左侧选择一个环境查看与修改变量。",
                        "Select an environment to view and edit its variables."
                    )),
            )
            .into_any_element();
    };

    let mut rows: Vec<AnyElement> = Vec::new();
    rows.push(
        h_flex()
            .gap_2()
            .justify_between()
            .items_center()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().text_sm().child(detail.name.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(detail.path.clone()),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(if detail.is_active {
                        Button::new("env-deactivate")
                            .outline()
                            .small()
                            .label(tr!(cx, "停用", "Deactivate"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.activate_environment(None, window, cx);
                            }))
                    } else {
                        Button::new("env-activate")
                            .primary()
                            .small()
                            .label(tr!(cx, "启用此环境", "Activate"))
                            .on_click({
                                let path = detail.path.clone();
                                cx.listener(move |this, _, window, cx| {
                                    this.activate_environment(Some(path.clone()), window, cx);
                                })
                            })
                    })
                    .child({
                        let path = detail.path.clone();
                        Button::new("env-delete")
                            .outline()
                            .small()
                            .danger()
                            .icon(IconName::Delete)
                            .label(tr!(cx, "删除环境", "Delete environment"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_environment(path.clone(), window, cx);
                            }))
                    }),
            )
            .into_any_element(),
    );

    rows.push(
        div()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .pt_2()
            .child(tr!(cx, "变量", "Variables"))
            .into_any_element(),
    );

    if detail.variables.is_empty() {
        rows.push(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(tr!(
                    cx,
                    "还没有变量。用下面的表单添加。",
                    "No variables yet. Add one with the form below."
                ))
                .into_any_element(),
        );
    } else {
        for (ix, variable) in detail.variables.iter().enumerate() {
            let field = this.env_fields.get(ix);
            let name_input = field.map(|field| field.name_input.clone());
            let value_input = field.map(|field| field.value_input.clone());
            rows.push(
                h_flex()
                    .id(("env-var", ix))
                    .gap_2()
                    .items_center()
                    .py_1()
                    .child(match name_input {
                        Some(input) => div()
                            .w(px(160.))
                            .flex_shrink_0()
                            .child(Input::new(&input).small())
                            .into_any_element(),
                        None => div()
                            .text_xs()
                            .w(px(160.))
                            .flex_shrink_0()
                            .font_family(theme.mono_font_family.clone())
                            .child(variable.key.clone())
                            .into_any_element(),
                    })
                    .child(match value_input {
                        Some(input) => div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&input).small())
                            .into_any_element(),
                        None => div()
                            .text_xs()
                            .flex_1()
                            .text_ellipsis()
                            .child(variable.value.clone())
                            .into_any_element(),
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .flex_shrink_0()
                            .child(if variable.enabled {
                                tr!(cx, "启用", "On")
                            } else {
                                tr!(cx, "停用", "Off")
                            }),
                    )
                    .child(
                        div()
                            .id(("env-var-remove", ix))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size_5()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|this| this.bg(theme.muted))
                            .child(gpui_kit::component::Icon::new(IconName::Delete).xsmall())
                            .on_click(cx.listener(move |this, _: &gpui_kit::ClickEvent, window, cx| {
                                this.delete_env_variable(ix, window, cx);
                            })),
                    )
                    .into_any_element(),
            );
        }
    }

    // 新增/修改变量的内联表单（简单：键 + 值 + 机密开关 + 提交）。
    rows.push(variable_form(this, cx));

    v_flex()
        .id("env-detail")
        .overflow_y_scroll()
        .size_full()
        .gap_1()
        .children(rows)
        .into_any_element()
}

fn variable_form(this: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let key = this.env_key.clone();
    let value = this.env_value.clone();
    h_flex()
        .gap_2()
        .items_center()
        .pt_2()
        .child(div().w(px(160.)).child(Input::new(&key).small()))
        .child(div().flex_1().child(Input::new(&value).small()))
        .child(
            Button::new("env-var-save")
                .primary()
                .small()
                .icon(IconName::Plus)
                .label(tr!(cx, "保存变量", "Save variable"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.save_env_variable(window, cx);
                })),
        )
        .into_any_element()
}

impl AppView {
    pub fn reload_environment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 重载会重建变量输入框。先把还没落盘的修改提交到当前环境
        // （此刻详情还指向旧环境），否则它们会被重建悄悄丢掉。
        self.flush_env_edits(window, cx);
        let Some(root) = self.workspace.read(cx).root() else {
            self.environment.detail = None;
            self.env_fields.clear();
            return;
        };
        let path = self.environment.selected_path.clone().or_else(|| {
            self.workspace
                .read(cx)
                .index()
                .and_then(|index| index.environments.first().map(|env| env.path.clone()))
        });
        let Some(path) = path else {
            self.environment.detail = None;
            self.env_fields.clear();
            return;
        };
        self.environment.selected_path = Some(path.clone());

        let core = self.core.clone();
        // 传入当前激活的环境，详情页才知道该显示「停用」还是「启用此环境」。
        let active = self
            .core
            .workspace
            .config()
            .ok()
            .and_then(|config| config.active_environment);
        let loaded_path = path.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::environment_detail(&root, &path, active.as_deref())
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| {
                // 快速连点几个环境时会有多个加载在飞，只接受仍然被选中的那个，
                // 否则旧的加载结果会把新选中的环境覆盖掉。
                if view.environment.selected_path.as_deref() == Some(loaded_path.as_str()) {
                    view.environment.detail = result.ok();
                    view.sync_env_fields(window, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn select_environment(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .workspace
            .read(cx)
            .index()
            .and_then(|index| index.environments.get(ix).map(|env| env.path.clone()))
        else {
            return;
        };
        self.environment.selected_path = Some(path);
        self.reload_environment(window, cx);
    }

    pub fn create_environment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let core = self.core.clone();
        let stamp = chrono::Local::now().format("%H%M%S");
        let name = tr!(cx, format!("环境 {stamp}"), format!("Environment {stamp}"));
        let receiver = core
            .spawn(async move { thread_core::workspace::ops::create_environment(&root, &name) });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok((path, _)) => {
                    view.workspace.update(cx, |model, cx| model.refresh(cx));
                    view.environment.selected_path = Some(path);
                    window.push_notification(
                        (
                            NotificationType::Success,
                            tr!(cx, "环境已创建。", "Environment created."),
                        ),
                        cx,
                    );
                }
                Err(error) => {
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    pub fn activate_environment(
        &mut self,
        identifier: Option<String>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let core = self.core.clone();
        let task_core = self.core.clone();
        let receiver =
            core.spawn(async move { task_core.set_active_environment(identifier.as_deref()) });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok(snapshot) => {
                    view.workspace.update(cx, |model, cx| {
                        model.snapshot = Some(snapshot);
                        cx.emit(crate::state::WorkspaceEvent::Changed);
                        cx.notify();
                    });
                    view.reload_environment(window, cx);
                    window.push_notification(tr!(cx, "环境已切换。", "Environment switched."), cx);
                }
                Err(error) => {
                    window.push_notification(
                        (
                            gpui_kit::component::notification::NotificationType::Error,
                            error.to_string(),
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub fn delete_environment(&mut self, path: String, _: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let selected = self.environment.selected_path.as_deref() == Some(path.as_str());
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::delete_environment(
                &root,
                &path,
                &thread_core::db::trash_dir(),
            )
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| {
                if selected && result.is_ok() {
                    view.environment.selected_path = None;
                    view.environment.detail = None;
                    view.env_fields.clear();
                }
                view.workspace.update(cx, |model, cx| model.refresh(cx));
                match result {
                    Ok(_) => window.push_notification(
                        tr!(
                            cx,
                            "环境已删除（文件移入本地回收站）。",
                            "Environment deleted. The file was moved to the local trash."
                        ),
                        cx,
                    ),
                    Err(error) => window.push_notification(
                        (
                            gpui_kit::component::notification::NotificationType::Error,
                            error.to_string(),
                        ),
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    pub fn rename_environment(
        &mut self,
        path: String,
        name: String,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let selected = self.environment.selected_path.as_deref() == Some(path.as_str());
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::update_environment(&root, &path, Some(name))
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok(_) => {
                    view.workspace.update(cx, |model, cx| model.refresh(cx));
                    if selected {
                        view.reload_environment(window, cx);
                    }
                    window.push_notification(tr!(cx, "环境已重命名。", "Environment renamed."), cx);
                }
                Err(error) => {
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    /// 按当前环境详情重建名称和值的输入框。列表没变时保留正在编辑的输入框。
    pub fn sync_env_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows: Vec<(String, String, bool, bool)> = self
            .environment
            .detail
            .as_ref()
            .map(|detail| {
                detail
                    .variables
                    .iter()
                    .map(|variable| {
                        (
                            variable.key.clone(),
                            variable.value.clone(),
                            variable.secret,
                            variable.has_stored_value,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let unchanged =
            self.env_fields.len() == rows.len()
                && self.env_fields.iter().zip(&rows).all(
                    |(field, (key, value, secret, stored))| {
                        field.key == *key
                            && field.value == *value
                            && field.secret == *secret
                            && field.stored == *stored
                    },
                );
        if unchanged {
            return;
        }
        self.env_fields.clear();
        for (key, value, secret, stored) in rows {
            let name_input = cx.new(|cx| InputState::new(window, cx).default_value(key.clone()));
            let value_placeholder = if secret {
                if stored {
                    tr!(cx, "输入新值以替换", "Type a new value to replace").to_string()
                } else {
                    tr!(cx, "机密值", "Secret value").to_string()
                }
            } else {
                tr!(cx, "值", "Value").to_string()
            };
            let value_input = cx.new(|cx| {
                let state = InputState::new(window, cx)
                    .placeholder(value_placeholder)
                    .default_value(if secret { String::new() } else { value.clone() });
                if secret { state.masked(true) } else { state }
            });
            let watched_name = name_input.clone();
            let name_watch = cx.subscribe_in(
                &watched_name,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.commit_env_name(input, window, cx);
                    }
                },
            );
            let watched_value = value_input.clone();
            let value_watch = cx.subscribe_in(
                &watched_value,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    match event {
                        // 打字就自动保存（防抖）；机密值不自动保存，避免半截密码落盘。
                        InputEvent::Change => this.schedule_env_value_save(input, cx),
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            this.commit_env_value(input, window, cx, true);
                        }
                        _ => {}
                    }
                },
            );
            self.env_fields.push(EnvVariableField {
                key,
                value: if secret { String::new() } else { value },
                secret,
                stored,
                name_input,
                value_input,
                name_watch,
                value_watch,
                pending_save: None,
            });
        }
    }

    /// 失焦或回车时把变量名写回。空名称会恢复，重名会提示并恢复。
    pub fn commit_env_name(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self
            .env_fields
            .iter()
            .position(|field| field.name_input == *input)
        else {
            return;
        };
        let from = self.env_fields[ix].key.clone();
        let to = input.read(cx).value().trim().to_string();
        if to.is_empty() || to == from {
            if input.read(cx).value() != from {
                input.update(cx, |state, cx| state.set_value(&from, window, cx));
            }
            return;
        }
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let Some(path) = self
            .environment
            .detail
            .as_ref()
            .map(|detail| detail.path.clone())
        else {
            return;
        };
        if input.read(cx).value() != to {
            input.update(cx, |state, cx| state.set_value(&to, window, cx));
        }
        self.env_fields[ix].key = to.clone();
        let saved_from = from.clone();
        let saved_to = to.clone();
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::rename_env_variable(&root, &path, &saved_from, &saved_to)
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok(_) => {
                    view.workspace.update(cx, |model, cx| model.refresh(cx));
                    view.reload_environment(window, cx);
                    window.push_notification(tr!(cx, "变量名已更新。", "Variable renamed."), cx);
                }
                Err(error) => {
                    if let Some(field) = view.env_fields.iter_mut().find(|field| field.key == to) {
                        field.key = from.clone();
                        field
                            .name_input
                            .update(cx, |state, cx| state.set_value(&from, window, cx));
                    }
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    /// 值输入变化后安排一次防抖自动保存。继续输入会重置计时；
    /// 机密值不自动保存（只在回车 / 失焦 / 冲刷时提交），避免半截密码落盘。
    fn schedule_env_value_save(&mut self, input: &Entity<InputState>, cx: &mut Context<Self>) {
        let Some(ix) = self
            .env_fields
            .iter()
            .position(|field| field.value_input == *input)
        else {
            return;
        };
        if self.env_fields[ix].secret {
            return;
        }
        let value_input = input.clone();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ENV_AUTOSAVE_DELAY).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.commit_env_value(&value_input, window, cx, false);
            });
        });
        self.env_fields[ix].pending_save = Some(task);
    }

    /// 立即提交还在输入中的变量名 / 值（防抖计时没到、又没有失焦的场景）。
    /// 必须在切换环境或重建输入框之前调用：此刻详情还指向旧环境，
    /// 提交会写到正确的文件里。
    pub fn flush_env_edits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_env_edits_except(None, window, cx);
    }

    /// 同上，但可以跳过一行（例如正要被删除的变量：先写它再删毫无意义，
    /// 而改名再删会让删除找不到新的键）。
    pub fn flush_env_edits_except(
        &mut self,
        skip: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.env_fields.is_empty() {
            return;
        }
        for field in &mut self.env_fields {
            field.pending_save = None;
        }
        let inputs: Vec<(Entity<InputState>, Entity<InputState>)> = self
            .env_fields
            .iter()
            .enumerate()
            .filter(|(ix, _)| skip != Some(*ix))
            .map(|(_, field)| (field.name_input.clone(), field.value_input.clone()))
            .collect();
        for (name_input, value_input) in inputs {
            let name_next = name_input.read(cx).value().trim().to_string();
            let value_next = value_input.read(cx).value().to_string();
            let name_changed = self
                .env_fields
                .iter()
                .find(|field| field.name_input == name_input)
                .is_some_and(|field| !name_next.is_empty() && name_next != field.key);
            let value_changed = self
                .env_fields
                .iter()
                .find(|field| field.value_input == value_input)
                .is_some_and(|field| {
                    value_next != field.value && !(field.secret && value_next.is_empty())
                });
            if name_changed && value_changed {
                // 名字和值同时变了：先按旧名写值再改名。两个写操作必须在
                // 同一个任务里按顺序执行，并发乱序会把旧名字的变量建回来。
                self.commit_env_name_and_value(&name_input, &name_next, &value_next, cx);
            } else if name_changed {
                self.commit_env_name(&name_input, window, cx);
            } else {
                self.commit_env_value(&value_input, window, cx, false);
            }
        }
    }

    /// 名字和值同时被修改：先按旧名写新值，再改名，同一个任务里按顺序执行。
    /// 失败时回滚字段状态；冲刷是静默的，出错只弹通知。
    fn commit_env_name_and_value(
        &mut self,
        name_input: &Entity<InputState>,
        to: &str,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self
            .env_fields
            .iter()
            .position(|field| field.name_input == *name_input)
        else {
            return;
        };
        let from = self.env_fields[ix].key.clone();
        let secret = self.env_fields[ix].secret;
        let previous_value = self.env_fields[ix].value.clone();
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let Some(detail) = self.environment.detail.as_ref() else {
            return;
        };
        let path = detail.path.clone();
        let enabled = detail
            .variables
            .iter()
            .find(|variable| variable.key == from)
            .is_none_or(|variable| variable.enabled);

        // 乐观更新字段；失败时回滚。
        self.env_fields[ix].key = to.to_string();
        self.env_fields[ix].value = if secret {
            String::new()
        } else {
            value.to_string()
        };

        let to = to.to_string();
        let value = value.to_string();
        let name_input = name_input.clone();
        let saved_from = from.clone();
        let saved_to = to.clone();
        let saved_value = value.clone();
        let saved_secret = secret;
        let ui_path = path.clone();
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            let saved = thread_core::workspace::ops::set_env_variable(
                &root,
                &path,
                &saved_from,
                Some(saved_value),
                Some(enabled),
                Some(saved_secret),
                None,
            );
            saved.and_then(|_| {
                thread_core::workspace::ops::rename_env_variable(
                    &root,
                    &path,
                    &saved_from,
                    &saved_to,
                )
            })
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok(_) => {
                    // 原地更新详情，避免重新加载打断正在输入的其它变量。
                    if let Some(detail) = view.environment.detail.as_mut()
                        && detail.path == ui_path
                        && let Some(variable) =
                            detail.variables.iter_mut().find(|variable| variable.key == from)
                    {
                        variable.key = to.clone();
                        if secret {
                            variable.has_stored_value = true;
                        } else {
                            variable.value = value.clone();
                        }
                    }
                }
                Err(error) => {
                    if let Some(field) = view
                        .env_fields
                        .iter_mut()
                        .find(|field| field.name_input == name_input)
                    {
                        field.key = from.clone();
                        field.value = previous_value.clone();
                    }
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    /// 提交一个变量的值。`notify` 为 true 时是显式提交（回车 / 失焦），
    /// 成功会弹通知、失败会恢复输入框；自动保存和切换前的冲刷是静默的，
    /// 失败时保留输入内容，下一次改动或失焦会重试。
    pub fn commit_env_value(
        &mut self,
        input: &Entity<InputState>,
        _window: &mut Window,
        cx: &mut Context<Self>,
        notify: bool,
    ) {
        let Some(ix) = self
            .env_fields
            .iter()
            .position(|field| field.value_input == *input)
        else {
            return;
        };
        let previous = self.env_fields[ix].value.clone();
        let secret = self.env_fields[ix].secret;
        let next = input.read(cx).value().to_string();
        if next == previous || (secret && next.is_empty()) {
            return;
        }
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let Some(detail) = self.environment.detail.as_ref() else {
            return;
        };
        let path = detail.path.clone();
        let key = self.env_fields[ix].key.clone();
        let enabled = detail
            .variables
            .iter()
            .find(|variable| variable.key == key)
            .is_none_or(|variable| variable.enabled);
        self.env_fields[ix].value = if secret { String::new() } else { next.clone() };
        let value_input = input.clone();
        let saved_value = next.clone();
        let ui_path = path.clone();
        let ui_key = key.clone();
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::set_env_variable(
                &root,
                &path,
                &key,
                Some(next),
                Some(enabled),
                Some(secret),
                None,
            )
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok(_) => {
                    if let Some(field) = view
                        .env_fields
                        .iter_mut()
                        .find(|field| field.value_input == value_input)
                        && field.secret
                    {
                        field.stored = true;
                        field
                            .value_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                    }
                    // 原地更新详情。重新加载会在保存还在线时重建输入框，
                    // 打断正在输入的其它变量。
                    if let Some(detail) = view.environment.detail.as_mut()
                        && detail.path == ui_path
                        && let Some(variable) = detail
                            .variables
                            .iter_mut()
                            .find(|variable| variable.key == ui_key)
                    {
                        if secret {
                            variable.has_stored_value = true;
                        } else {
                            variable.value = saved_value.clone();
                        }
                    }
                    if notify {
                        window
                            .push_notification(tr!(cx, "变量值已更新。", "Variable value saved."), cx);
                    }
                }
                Err(error) => {
                    if let Some(field) = view
                        .env_fields
                        .iter_mut()
                        .find(|field| field.value_input == value_input)
                    {
                        field.value = previous.clone();
                        if notify && !field.secret {
                            let restore = previous.clone();
                            field.value_input.update(cx, |state, cx| {
                                state.set_value(restore, window, cx);
                            });
                        }
                    }
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    pub fn save_env_variable(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let Some(detail) = &self.environment.detail else {
            return;
        };
        let path = detail.path.clone();
        let key = self.env_key.read(cx).value().trim().to_string();
        let value = self.env_value.read(cx).value().to_string();
        if key.is_empty() {
            return;
        }
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::set_env_variable(
                &root,
                &path,
                &key,
                Some(value),
                Some(true),
                Some(false),
                None,
            )
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok(_) => {
                    view.env_key
                        .update(cx, |state, cx| state.set_value("", window, cx));
                    view.env_value
                        .update(cx, |state, cx| state.set_value("", window, cx));
                    view.reload_environment(window, cx);
                    window.push_notification(tr!(cx, "变量已保存。", "Variable saved."), cx);
                }
                Err(error) => {
                    window.push_notification(
                        (
                            gpui_kit::component::notification::NotificationType::Error,
                            error.to_string(),
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub fn delete_env_variable(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let Some(detail) = &self.environment.detail else {
            return;
        };
        let Some(variable) = detail.variables.get(ix) else {
            return;
        };
        let path = detail.path.clone();
        let key = variable.key.clone();
        // 先把其它行还在输入中的修改落盘，并取消所有防抖任务；否则自动保存
        // 可能在删除之后触发，把删掉的变量重新建回来。正要删除的这一行
        // 跳过（写了也马上删，改名还会让删除找不到键）。
        self.flush_env_edits_except(Some(ix), window, cx);
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::delete_env_variable(&root, &path, &key)
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok(_) => view.reload_environment(window, cx),
                Err(error) => {
                    tracing::warn!(?error, "failed to delete variable");
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }
}
