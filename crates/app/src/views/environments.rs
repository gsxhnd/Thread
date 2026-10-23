//! 环境视图：环境列表 + 变量编辑 + 激活。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _,
    SharedString, Styled as _, Subscription, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::error::AppError;
use thread_core::workspace::index::EnvironmentSummary;

use crate::app::AppView;
use crate::widgets;

/// 环境变量名称的输入框。订阅先于输入框释放，避免销毁时的失焦再提交一次。
pub struct EnvNameField {
    original: String,
    /// 持有输入框订阅。字段本身不读，丢掉这一行时订阅一起取消。
    #[expect(dead_code)]
    watch: Subscription,
    input: Entity<InputState>,
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
        return div()
            .flex_1()
            .pt_8()
            .child(widgets::compact_empty(
                cx,
                IconName::Braces,
                tr!(cx, "还没有环境。", "No environments yet."),
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
                    v_flex()
                        .id(SharedString::from(format!("env-item-{ix}")))
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
                        .into_any_element()
                })),
        )
        .into_any_element()
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
                    .child(
                        Button::new("env-delete")
                            .outline()
                            .small()
                            .danger()
                            .icon(IconName::Delete)
                            .label(tr!(cx, "删除环境", "Delete environment"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delete_environment(window, cx);
                            })),
                    ),
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
            let name_input = this.env_names.get(ix).map(|field| field.input.clone());
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
                    .child(
                        div()
                            .text_xs()
                            .flex_1()
                            .text_ellipsis()
                            .child(if variable.secret {
                                if variable.has_stored_value {
                                    tr!(
                                        cx,
                                        "••••••（机密，已存本地）",
                                        "•••••• (secret, stored locally)"
                                    )
                                    .to_string()
                                } else {
                                    tr!(cx, "••••••（机密，未设置值）", "•••••• (secret, no value)")
                                        .to_string()
                                }
                            } else {
                                variable.value.clone()
                            }),
                    )
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
                            .on_click(cx.listener(move |this, _: &gpui_kit::ClickEvent, _, cx| {
                                this.delete_env_variable(ix, cx);
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
    pub fn reload_environment(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            self.environment.detail = None;
            self.env_names.clear();
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
            self.env_names.clear();
            return;
        };
        self.environment.selected_path = Some(path.clone());

        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::environment_detail(&root, &path, None)
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| {
                view.environment.detail = result.ok();
                view.sync_env_names(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn select_environment(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .workspace
            .read(cx)
            .index()
            .and_then(|index| index.environments.get(ix).map(|env| env.path.clone()))
        else {
            return;
        };
        self.environment.selected_path = Some(path);
        self.reload_environment(cx);
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
                    view.reload_environment(cx);
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

    pub fn delete_environment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let Some(detail) = &self.environment.detail else {
            return;
        };
        let path = detail.path.clone();
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
                view.environment.selected_path = None;
                view.environment.detail = None;
                view.env_names.clear();
                view.workspace.update(cx, |model, cx| {
                    let snapshot = model.core.workspace.snapshot().ok();
                    model.snapshot = snapshot;
                    cx.emit(crate::state::WorkspaceEvent::Changed);
                    cx.notify();
                });
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

    /// 按当前环境详情重建名称输入框。名称列表没变时保留正在编辑的输入框。
    pub fn sync_env_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys: Vec<String> = self
            .environment
            .detail
            .as_ref()
            .map(|detail| {
                detail
                    .variables
                    .iter()
                    .map(|variable| variable.key.clone())
                    .collect()
            })
            .unwrap_or_default();
        let unchanged = self.env_names.len() == keys.len()
            && self
                .env_names
                .iter()
                .zip(&keys)
                .all(|(field, key)| field.original == *key);
        if unchanged {
            return;
        }
        self.env_names.clear();
        for key in keys {
            let input = cx.new(|cx| InputState::new(window, cx).default_value(key.clone()));
            let watched = input.clone();
            let watch = cx.subscribe_in(
                &watched,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.commit_env_name(input, window, cx);
                    }
                },
            );
            self.env_names.push(EnvNameField {
                original: key,
                watch,
                input,
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
            .env_names
            .iter()
            .position(|field| field.input == *input)
        else {
            return;
        };
        let from = self.env_names[ix].original.clone();
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
        self.env_names[ix].original = to.clone();
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
                    view.reload_environment(cx);
                    window.push_notification(tr!(cx, "变量名已更新。", "Variable renamed."), cx);
                }
                Err(error) => {
                    if let Some(field) =
                        view.env_names.iter_mut().find(|field| field.original == to)
                    {
                        field.original = from.clone();
                        field
                            .input
                            .update(cx, |state, cx| state.set_value(&from, window, cx));
                    }
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    pub fn save_env_variable(&mut self, _: &mut Window, cx: &mut Context<Self>) {
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
                    view.reload_environment(cx);
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

    pub fn delete_env_variable(&mut self, ix: usize, cx: &mut Context<Self>) {
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
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::delete_env_variable(&root, &path, &key)
        });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update(cx, |view, cx| match result {
                Ok(_) => view.reload_environment(cx),
                Err(error) => {
                    tracing::warn!(?error, "failed to delete variable");
                }
            });
        })
        .detach();
    }
}
