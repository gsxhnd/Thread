//! 侧边栏：工作区身份 + 视图导航 + 集合树。

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::tree::{TreeItem, tree};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, FontWeight, IntoElement, MouseButton, ParentElement as _,
    PromptLevel, Render, SharedString, Styled as _, WeakEntity, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::error::{AppError, AppResult};
use thread_core::workspace::index::{CollectionNode, RequestNode, WorkspaceIndex};

use crate::app::AppView;
use crate::state::MainView;

/// 由索引构建集合树（集合可嵌套，请求作为叶子）。
fn build_items(
    collections: &[CollectionNode],
    collapsed: &std::collections::HashSet<String>,
) -> Vec<TreeItem> {
    collections
        .iter()
        .map(|node| collection_item(node, collapsed))
        .collect()
}

fn collection_item(
    node: &CollectionNode,
    collapsed: &std::collections::HashSet<String>,
) -> TreeItem {
    let mut item = TreeItem::new(
        SharedString::from(format!("coll:{}", node.path)),
        node.name.clone(),
    )
    .expanded(!collapsed.contains(&node.path));
    for request in &node.requests {
        item = item.child(request_item(request));
    }
    for child in &node.children {
        item = item.child(collection_item(child, collapsed));
    }
    item
}

fn request_item(node: &RequestNode) -> TreeItem {
    TreeItem::new(
        SharedString::from(format!("req:{}", node.path)),
        node.name.clone(),
    )
}

/// 打开/重建侧边栏树（工作区变化时调用）。
pub fn rebuild_tree(this: &mut AppView, cx: &mut Context<AppView>) {
    let items = this
        .workspace
        .read(cx)
        .index()
        .map(|index| build_items(&index.collections, &this.collapsed_collections))
        .unwrap_or_default();
    this.tree.update(cx, |state, cx| state.set_items(items, cx));
}

pub fn render(
    this: &mut AppView,
    _window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let index = this.workspace.read(cx).index().cloned();
    let request_count = index.as_ref().map(|index| index.request_count).unwrap_or(0);
    let collection_count = index
        .as_ref()
        .map(|index| index.collection_count)
        .unwrap_or(0);
    let active_env = index
        .as_ref()
        .and_then(|index| index.active_environment.clone());
    let weak: WeakEntity<AppView> = cx.entity().downgrade();
    let menu_weak = weak.clone();
    let header_weak = weak.clone();
    let open_collection = this.open_collection.clone();
    let renaming_id = this.rename.as_ref().map(|rename| rename.node_id.clone());
    let renaming_input = this.rename.as_ref().map(|rename| rename.input.clone());

    let tree = tree(&this.tree, move |ix, entry, selected, _, _| {
        let item = entry.item();
        let id = item.id.clone();
        let is_request = id.starts_with("req:");
        let icon = if is_request {
            if id.ends_with(".grpc.json") {
                IconName::Boxes
            } else if id.ends_with(".ws.json") {
                IconName::Radio
            } else if id.ends_with(".graphql.json") {
                IconName::Share2
            } else if id.ends_with(".mqtt.json") {
                IconName::Satellite
            } else {
                IconName::Send
            }
        } else if entry.is_expanded() {
            IconName::FolderOpen
        } else {
            IconName::Folder
        };

        let folder = !is_request && entry.is_folder();
        let expanded = entry.is_expanded();
        let open_here = open_collection.as_deref() == id.strip_prefix("coll:");
        let editing = renaming_id.as_deref() == Some(id.as_ref());
        let mut row = ListItem::new(ix)
            .selected(selected || open_here)
            .pl(px(8.) + px(12.) * entry.depth() as f32)
            .child(
                h_flex()
                    .gap_2()
                    .min_w_0()
                    .flex_1()
                    .when(folder, |row| {
                        let toggle_id = id.clone();
                        let toggle_weak = weak.clone();
                        row.child(
                            div()
                                .id(SharedString::from(format!("coll-toggle-{id}")))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size_4()
                                .rounded_sm()
                                .cursor_pointer()
                                .child(
                                    Icon::new(if expanded {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .xsmall(),
                                )
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    let Some(path) = toggle_id.strip_prefix("coll:") else {
                                        return;
                                    };
                                    let path = path.to_string();
                                    let Some(view) = toggle_weak.upgrade() else {
                                        return;
                                    };
                                    view.update(cx, |view, cx| {
                                        if !view.collapsed_collections.remove(&path) {
                                            view.collapsed_collections.insert(path);
                                        }
                                        rebuild_tree(view, cx);
                                    });
                                })
                                .on_click(|_, _, cx| {
                                    cx.stop_propagation();
                                }),
                        )
                    })
                    .child(Icon::new(icon).xsmall())
                    .child(if editing {
                        renaming_input
                            .clone()
                            .map(|input| {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(Input::new(&input).small())
                                    .into_any_element()
                            })
                            .unwrap_or_else(|| {
                                div()
                                    .text_sm()
                                    .text_ellipsis()
                                    .child(item.label.clone())
                                    .into_any_element()
                            })
                    } else {
                        div()
                            .text_sm()
                            .text_ellipsis()
                            .child(item.label.clone())
                            .into_any_element()
                    }),
            );

        if is_request && !editing {
            let weak = weak.clone();
            let click_id = id.clone();
            row = row.on_click(move |_, window, cx| {
                let Some(path) = click_id.strip_prefix("req:") else {
                    return;
                };
                let Some(view) = weak.upgrade() else {
                    return;
                };
                view.update(cx, |view, cx| {
                    let node = view
                        .workspace
                        .read(cx)
                        .index()
                        .and_then(|index| find_request(index, path));
                    if let Some(node) = node {
                        view.open_request(&node, window, cx);
                    } else {
                        window.push_notification(
                            tr!(
                                cx,
                                "找不到该请求，可能已被外部删除。",
                                "That request is gone. It may have been deleted outside Thread."
                            ),
                            cx,
                        );
                    }
                });
            });
            let drag_path = id.strip_prefix("req:").unwrap_or_default().to_string();
            row = row.on_drag(
                RequestDrag {
                    path: drag_path,
                    name: item.label.clone(),
                },
                |drag, _, _, cx| {
                    cx.new(|_| DragLabel {
                        name: drag.name.clone(),
                    })
                },
            );
        } else if !editing {
            let weak = weak.clone();
            let click_id = id.clone();
            row = row
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_click(move |_, _, cx| {
                    let Some(path) = click_id.strip_prefix("coll:") else {
                        return;
                    };
                    let path = path.to_string();
                    let Some(view) = weak.upgrade() else {
                        return;
                    };
                    view.update(cx, |view, cx| view.open_collection(path, cx));
                });
        }
        let drop_weak = weak.clone();
        let drop_id = id.clone();
        let hover_id = id.clone();
        row = row
            .can_drop(|value: &dyn std::any::Any, _, _| value.is::<RequestDrag>())
            .drag_over::<RequestDrag>(move |style, drag, _, cx| {
                let theme = cx.theme();
                if hover_id.starts_with("coll:") {
                    style
                        .rounded_md()
                        .bg(theme.drop_target)
                        .border_l_4()
                        .border_color(theme.drag_border)
                } else if hover_id.strip_prefix("req:") == Some(drag.path.as_str()) {
                    style
                } else {
                    style.border_t_4().border_color(theme.drag_border)
                }
            })
            .on_drop(move |drag: &RequestDrag, window, cx| {
                let Some(view) = drop_weak.upgrade() else {
                    return;
                };
                let drag = drag.clone();
                view.update(cx, |view, cx| {
                    view.place_dragged_request(drag, &drop_id, window, cx);
                });
            });
        row
    })
    .context_menu(move |_ix, entry, menu, _window, cx| {
        node_context_menu(menu, entry.item().id.clone(), menu_weak.clone(), cx)
    });

    let nav = h_flex().px_2().pt_2().pb_1().gap_0p5().children(
        MainView::ALL
            .iter()
            .map(|view| nav_button(cx, this.view == *view, *view)),
    );

    v_flex()
        .id("sidebar")
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .bg(theme.sidebar)
        .text_color(theme.sidebar_foreground)
        .on_action(
            cx.listener(|this, _: &gpui_kit::component::input::Escape, _, cx| {
                this.cancel_inline_rename(cx);
            }),
        )
        // 导航收成一排图标，把高度留给集合树。名称在悬停提示里。
        .child(nav)
        .child(div().mx_3().border_t_1().border_color(theme.border))
        // 集合树
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .px_3()
                .pt_2()
                .pb_1()
                .child(
                    div()
                        .id("collection-heading")
                        .flex_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.muted_foreground)
                        .child(tr!(cx, "集合", "Collections"))
                        .context_menu(move |menu, _window, cx| {
                            root_collection_menu(menu, header_weak.clone(), cx)
                        }),
                )
                .child(
                    h_flex()
                        .gap_0p5()
                        .child(draft_icon(
                            cx,
                            "draft-http",
                            IconName::Send,
                            tr!(cx, "新建 HTTP", "New HTTP"),
                            crate::views::request::Protocol::Http,
                        ))
                        .child(draft_icon(
                            cx,
                            "draft-gql",
                            IconName::Share2,
                            tr!(cx, "新建 GraphQL", "New GraphQL"),
                            crate::views::request::Protocol::Graphql,
                        ))
                        .child(draft_icon(
                            cx,
                            "draft-ws",
                            IconName::Radio,
                            tr!(cx, "新建 WebSocket", "New WebSocket"),
                            crate::views::request::Protocol::Websocket,
                        ))
                        .child(draft_icon(
                            cx,
                            "draft-grpc",
                            IconName::Boxes,
                            tr!(cx, "新建 gRPC", "New gRPC"),
                            crate::views::request::Protocol::Grpc,
                        )),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .id("collection-tree")
                .overflow_y_scroll()
                .px_2()
                .child(tree),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(tr!(
                    cx,
                    format!("{collection_count} 个集合 · {request_count} 个请求"),
                    format!("{collection_count} collections · {request_count} requests")
                ))
                .when_some(active_env, |row, env| {
                    row.child(div().child("·")).child(
                        h_flex()
                            .gap_1()
                            .min_w_0()
                            .child(Icon::new(IconName::Braces).xsmall())
                            .child(div().text_ellipsis().child(env)),
                    )
                }),
        )
}

fn draft_icon(
    cx: &mut gpui_kit::Context<AppView>,
    id: &'static str,
    icon: IconName,
    tip: &'static str,
    protocol: crate::views::request::Protocol,
) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .xsmall()
        .icon(icon)
        .tooltip(tip)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.new_draft_tab_for(protocol, window, cx);
        }))
}

fn nav_button(cx: &mut Context<AppView>, active: bool, view: MainView) -> gpui_kit::AnyElement {
    Button::new(nav_id(view))
        .ghost()
        .small()
        .icon(icon_for(view))
        .selected(active)
        .tooltip(view.label(cx))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.switch_view(view, window, cx);
        }))
        .into_any_element()
}

fn nav_id(view: MainView) -> &'static str {
    match view {
        MainView::Request => "nav-request",
        MainView::Environments => "nav-environments",
        MainView::History => "nav-history",
        MainView::Proxy => "nav-proxy",
        MainView::Certificates => "nav-certificates",
    }
}

fn icon_for(view: MainView) -> IconName {
    match view {
        MainView::Request => IconName::Send,
        MainView::History => IconName::Clock,
        MainView::Proxy => IconName::Network,
        MainView::Certificates => IconName::ShieldCheck,
        MainView::Environments => IconName::Braces,
    }
}

fn node_context_menu(
    menu: PopupMenu,
    id: SharedString,
    weak: WeakEntity<AppView>,
    cx: &App,
) -> PopupMenu {
    if let Some(path) = id.strip_prefix("coll:") {
        let path = path.to_string();
        return menu
            .item(new_request_item(
                tr!(cx, "新建 HTTP 请求", "New HTTP request"),
                "http",
                &path,
                weak.clone(),
            ))
            .item(new_request_item(
                tr!(cx, "新建 GraphQL 请求", "New GraphQL request"),
                "graphql",
                &path,
                weak.clone(),
            ))
            .item(new_request_item(
                tr!(cx, "新建 WebSocket 请求", "New WebSocket request"),
                "websocket",
                &path,
                weak.clone(),
            ))
            .item(new_request_item(
                tr!(cx, "新建 gRPC 请求", "New gRPC request"),
                "grpc",
                &path,
                weak.clone(),
            ))
            .separator()
            .item(new_collection_item(
                tr!(cx, "新建子集合", "New subcollection"),
                Some(path.clone()),
                weak.clone(),
            ))
            .item(rename_item(
                tr!(cx, "重命名", "Rename"),
                path.clone(),
                false,
                weak.clone(),
            ))
            .separator()
            .item(trash_item(
                tr!(cx, "移到回收站", "Move to trash"),
                path,
                false,
                weak,
            ));
    }
    if let Some(path) = id.strip_prefix("req:") {
        let path = path.to_string();
        return menu
            .item(open_request_item(
                tr!(cx, "打开", "Open"),
                path.clone(),
                weak.clone(),
            ))
            .item(rename_item(
                tr!(cx, "重命名", "Rename"),
                path.clone(),
                true,
                weak.clone(),
            ))
            .separator()
            .item(trash_item(
                tr!(cx, "移到回收站", "Move to trash"),
                path,
                true,
                weak,
            ));
    }
    menu
}

fn root_collection_menu(menu: PopupMenu, weak: WeakEntity<AppView>, cx: &App) -> PopupMenu {
    menu.item(new_collection_item(
        tr!(cx, "新建集合", "New collection"),
        None,
        weak,
    ))
}

fn open_request_item(
    label: &'static str,
    path: String,
    weak: WeakEntity<AppView>,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, _, cx| {
        let path = path.clone();
        let _ = weak.update_in(cx, |view, window, cx| {
            let node = view
                .workspace
                .read(cx)
                .index()
                .and_then(|index| find_request(index, &path));
            if let Some(node) = node {
                view.open_request(&node, window, cx);
            } else {
                window.push_notification(
                    tr!(
                        cx,
                        "找不到该请求，可能已被外部删除。",
                        "That request is gone. It may have been deleted outside Thread."
                    ),
                    cx,
                );
            }
        });
    })
}

fn new_request_item(
    label: &'static str,
    protocol: &'static str,
    collection: &str,
    weak: WeakEntity<AppView>,
) -> PopupMenuItem {
    let collection = collection.to_string();
    let protocol = protocol.to_string();
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let collection = collection.clone();
        let protocol = protocol.clone();
        let locale = crate::i18n::current(cx);
        prompt_name(
            window,
            cx,
            tr!(cx, "新建请求", "New request"),
            tr!(cx, "请求名称", "Request name"),
            weak.clone(),
            move |root, name| {
                thread_core::workspace::ops::create_request(&root, &collection, &name, &protocol)
                    .map(|(_, file)| {
                        let file_name = file.name().to_string();
                        crate::i18n::owned(
                            locale,
                            format!("已创建请求 {file_name}"),
                            format!("Created request {file_name}"),
                        )
                    })
            },
        );
    })
}

fn new_collection_item(
    label: &'static str,
    parent: Option<String>,
    weak: WeakEntity<AppView>,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let parent = parent.clone();
        let locale = crate::i18n::current(cx);
        prompt_name(
            window,
            cx,
            tr!(cx, "新建集合", "New collection"),
            tr!(cx, "集合名称", "Collection name"),
            weak.clone(),
            move |root, name| {
                thread_core::workspace::ops::create_collection(
                    &root,
                    parent.as_deref(),
                    &name,
                    None,
                )
                .map(|(_, meta)| {
                    let collection_name = meta.name.clone();
                    crate::i18n::owned(
                        locale,
                        format!("已创建集合 {collection_name}"),
                        format!("Created collection {collection_name}"),
                    )
                })
            },
        );
    })
}

fn rename_item(
    label: &'static str,
    path: String,
    request: bool,
    weak: WeakEntity<AppView>,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let path = path.clone();
        let _ = weak.update_in(cx, |view, window, cx| {
            let name = view.node_name(&path, request, cx).unwrap_or_default();
            view.begin_inline_rename(path, request, name, window, cx);
        });
        let _ = window;
    })
}

fn trash_item(
    label: &'static str,
    path: String,
    request: bool,
    weak: WeakEntity<AppView>,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let path = path.clone();
        let locale = crate::i18n::current(cx);
        let message = if request {
            tr!(
                cx,
                "把这个请求移到回收站？",
                "Move this request to the trash?"
            )
        } else {
            tr!(
                cx,
                "把这个集合和里面的请求移到回收站？",
                "Move this collection and its requests to the trash?"
            )
        };
        confirm_trash(window, cx, message, weak.clone(), move |root| {
            let trash = thread_core::db::trash_dir();
            let trashed = if request {
                thread_core::workspace::ops::delete_request(&root, &path, &trash)
            } else {
                thread_core::workspace::ops::delete_collection(&root, &path, &trash)
            };
            trashed.map(|trashed| {
                crate::i18n::owned(
                    locale,
                    format!("已移到回收站 {trashed}"),
                    format!("Moved to trash: {trashed}"),
                )
            })
        });
    })
}

/// 名称对话框。输入框在打开前创建，确认后才把写操作交给工作区模型。
fn prompt_name<F>(
    window: &mut Window,
    cx: &mut App,
    title: &'static str,
    placeholder: &'static str,
    weak: WeakEntity<AppView>,
    job: F,
) where
    F: FnOnce(PathBuf, String) -> AppResult<String> + Send + 'static,
{
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
    let job_slot = Rc::new(RefCell::new(Some(job)));
    let cancel = tr!(cx, "取消", "Cancel");
    let confirm = tr!(cx, "确定", "OK");
    window.open_dialog(cx, move |dialog, _, _| {
        let input = input.clone();
        let weak = weak.clone();
        let job_slot = job_slot.clone();
        dialog
            .title(title)
            .w(px(360.))
            .child(Input::new(&input))
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("collection-name-cancel")
                            .label(cancel)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("collection-name-ok")
                            .primary()
                            .label(confirm)
                            .on_click(move |_, window, cx| {
                                let name = input.read(cx).value().trim().to_string();
                                if name.is_empty() {
                                    window.push_notification(
                                        tr!(cx, "名称不能为空。", "Name cannot be empty."),
                                        cx,
                                    );
                                    return;
                                }
                                let Some(job) = job_slot.borrow_mut().take() else {
                                    return;
                                };
                                window.close_dialog(cx);
                                let _ = weak.update(cx, |view, cx| {
                                    view.workspace.update(cx, |model, cx| {
                                        model.run(cx, move |root| job(root, name));
                                    });
                                });
                            }),
                    ),
            )
    });
}

fn confirm_trash<F>(
    window: &mut Window,
    cx: &mut App,
    message: &'static str,
    weak: WeakEntity<AppView>,
    job: F,
) where
    F: FnOnce(PathBuf) -> AppResult<String> + Send + 'static,
{
    let receiver = window.prompt(
        PromptLevel::Warning,
        message,
        Some(tr!(
            cx,
            "文件会移到应用回收站，可以稍后找回。",
            "Files go to the app trash and can be restored later."
        )),
        &[
            tr!(cx, "移到回收站", "Move to trash"),
            tr!(cx, "取消", "Cancel"),
        ],
        cx,
    );
    cx.spawn(async move |cx| {
        if receiver.await == Ok(0) {
            let _ = weak.update(cx, |view, cx| {
                view.workspace.update(cx, |model, cx| model.run(cx, job));
            });
        }
    })
    .detach();
}

/// 从索引里解析请求节点。
pub fn find_request(index: &WorkspaceIndex, path: &str) -> Option<RequestNode> {
    index.find_request(path).map(|location| location.request)
}

#[derive(Clone)]
struct RequestDrag {
    path: String,
    name: SharedString,
}

struct DragLabel {
    name: SharedString,
}

impl Render for DragLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        h_flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.drag_border)
            .bg(theme.drop_target)
            .child(Icon::new(IconName::Send).xsmall())
            .child(div().text_sm().child(self.name.clone()))
    }
}

fn parent_rel(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|parent| !parent.is_empty() && parent != ".")
        .unwrap_or_default()
}

fn collection_request_paths(index: &WorkspaceIndex, collection: &str) -> Vec<String> {
    fn walk<'a>(nodes: &'a [CollectionNode], path: &str) -> Option<&'a CollectionNode> {
        for node in nodes {
            if node.path == path {
                return Some(node);
            }
            if let Some(found) = walk(&node.children, path) {
                return Some(found);
            }
        }
        None
    }
    walk(&index.collections, collection)
        .map(|node| {
            node.requests
                .iter()
                .map(|request| request.path.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn move_before(order: &mut Vec<String>, path: &str, before: &str) {
    order.retain(|item| item != path);
    let index = order
        .iter()
        .position(|item| item == before)
        .unwrap_or(order.len());
    order.insert(index, path.to_string());
}

impl AppView {
    /// 回车：请求在原位置改名，集合则展开或收起。
    pub fn rename_or_toggle_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.tree.read(cx).selected_item().cloned() else {
            return;
        };
        let id = item.id.to_string();
        if let Some(path) = id.strip_prefix("req:") {
            self.begin_inline_rename(path.to_string(), true, item.label.to_string(), window, cx);
        } else if let Some(path) = id.strip_prefix("coll:") {
            let path = path.to_string();
            if !self.collapsed_collections.remove(&path) {
                self.collapsed_collections.insert(path);
            }
            rebuild_tree(self, cx);
        }
    }

    pub fn node_name(&self, path: &str, request: bool, cx: &App) -> Option<String> {
        let index = self.workspace.read(cx).index()?;
        if request {
            return index
                .find_request(path)
                .map(|location| location.request.name);
        }
        fn walk(nodes: &[CollectionNode], path: &str) -> Option<String> {
            for node in nodes {
                if node.path == path {
                    return Some(node.name.clone());
                }
                if let Some(name) = walk(&node.children, path) {
                    return Some(name);
                }
            }
            None
        }
        walk(&index.collections, path)
    }

    /// 在集合树原来的那一行里编辑名称，不弹对话框。
    pub fn begin_inline_rename(
        &mut self,
        path: String,
        request: bool,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rename = None;
        let node_id = if request {
            format!("req:{path}")
        } else {
            format!("coll:{path}")
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let watched = input.clone();
        let watch = cx.subscribe_in(
            &watched,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_inline_rename(true, window, cx),
                InputEvent::Blur => {
                    let armed = this.rename.as_ref().is_some_and(|rename| rename.armed);
                    if armed {
                        this.commit_inline_rename(false, window, cx);
                    }
                }
                InputEvent::Change | InputEvent::Focus => {}
            },
        );
        self.rename = Some(crate::app::InlineRename {
            node_id,
            path,
            request,
            input: input.clone(),
            armed: false,
            watch,
        });
        cx.notify();

        let view = cx.entity().downgrade();
        let input_id = input.entity_id();
        // 右键菜单关掉时会把焦点抢回去。等两帧再聚焦，避免刚打开就被失焦提交。
        window.on_next_frame(move |window, _cx| {
            window.on_next_frame(move |window, cx| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                view.update(cx, |this, cx| {
                    let same = this
                        .rename
                        .as_ref()
                        .is_some_and(|rename| rename.input.entity_id() == input_id);
                    if !same {
                        return;
                    }
                    if let Some(rename) = this.rename.as_mut() {
                        rename.armed = true;
                    }
                    let Some(input) = this.rename.as_ref().map(|rename| rename.input.clone())
                    else {
                        return;
                    };
                    input.update(cx, |state, cx| {
                        state.select_all(window, cx);
                        state.focus(window, cx);
                    });
                });
            });
        });
    }

    pub fn cancel_inline_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename.take().is_some() {
            cx.notify();
        }
    }

    fn commit_inline_rename(
        &mut self,
        from_enter: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rename) = self.rename.take() else {
            return;
        };
        let name = rename.input.read(cx).value().trim().to_string();
        if name.is_empty() {
            if from_enter {
                self.rename = Some(rename);
                window.push_notification(tr!(cx, "名称不能为空。", "Name cannot be empty."), cx);
            }
            cx.notify();
            return;
        }
        if self.node_name(&rename.path, rename.request, cx).as_deref() == Some(name.as_str()) {
            cx.notify();
            return;
        }
        if rename.request {
            for tab in &mut self.tabs {
                if tab.path.as_deref() == Some(rename.path.as_str()) {
                    tab.name = name.clone();
                }
            }
        }
        let path = rename.path.clone();
        let request = rename.request;
        let locale = crate::i18n::current(cx);
        self.workspace.update(cx, |model, cx| {
            model.run(cx, move |root| {
                if request {
                    thread_core::workspace::ops::rename_request(&root, &path, &name).map(|file| {
                        let file_name = file.name().to_string();
                        crate::i18n::owned(
                            locale,
                            format!("已重命名为 {file_name}"),
                            format!("Renamed to {file_name}"),
                        )
                    })
                } else {
                    thread_core::workspace::ops::update_collection(&root, &path, Some(name), None)
                        .map(|meta| {
                            let collection_name = meta.name.clone();
                            crate::i18n::owned(
                                locale,
                                format!("已重命名为 {collection_name}"),
                                format!("Renamed to {collection_name}"),
                            )
                        })
                }
            });
        });
        cx.notify();
    }

    /// 拖拽请求：同一个集合里改顺序，丢到别的集合里就移动文件。
    fn place_dragged_request(
        &mut self,
        drag: RequestDrag,
        target_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (onto_request, target_path) = if let Some(path) = target_id.strip_prefix("req:") {
            if path == drag.path {
                return;
            }
            (true, path.to_string())
        } else if let Some(path) = target_id.strip_prefix("coll:") {
            (false, path.to_string())
        } else {
            return;
        };

        let source_collection = parent_rel(&drag.path);
        let dest_collection = if onto_request {
            parent_rel(&target_path)
        } else {
            target_path.clone()
        };
        let Some(index) = self.workspace.read(cx).index().cloned() else {
            return;
        };
        let mut source_order = collection_request_paths(&index, &source_collection);
        if source_collection == dest_collection {
            if onto_request {
                move_before(&mut source_order, &drag.path, &target_path);
            } else {
                source_order.retain(|path| path != &drag.path);
                source_order.push(drag.path.clone());
            }
            let collection = source_collection.clone();
            let locale = crate::i18n::current(cx);
            self.workspace.update(cx, |model, cx| {
                let order = source_order.clone();
                let collection = collection.clone();
                model.run(cx, move |root| {
                    thread_core::workspace::ops::set_request_order(&root, &collection, &order).map(
                        |_| {
                            crate::i18n::owned(
                                locale,
                                "已调整请求顺序。".to_string(),
                                "Request order updated.".to_string(),
                            )
                        },
                    )
                });
            });
            let _ = window;
            return;
        }

        let Some(root) = self
            .workspace
            .read(cx)
            .root()
            .map(|path| path.to_path_buf())
        else {
            return;
        };
        let workspace_key = root.to_string_lossy().into_owned();
        let old_path = drag.path.clone();
        let previous = old_path.clone();
        let dest_order = collection_request_paths(&index, &dest_collection);
        source_order.retain(|path| path != &old_path);
        let db = self.core.db.clone();
        let core = self.core.clone();
        let onto = onto_request;
        let target_for_insert = target_path.clone();
        let receiver = core.spawn(async move {
            let new_path =
                thread_core::workspace::ops::move_request_file(&root, &old_path, &dest_collection)?;
            let mut dest = dest_order;
            if onto {
                let index = dest
                    .iter()
                    .position(|path| path == &target_for_insert)
                    .unwrap_or(dest.len());
                dest.insert(index, new_path.clone());
            } else {
                dest.push(new_path.clone());
            }
            thread_core::workspace::ops::set_request_order(
                &root,
                &source_collection,
                &source_order,
            )?;
            thread_core::workspace::ops::set_request_order(&root, &dest_collection, &dest)?;
            let _ = thread_core::services::history::retarget_request(
                &db,
                &workspace_key,
                &old_path,
                &new_path,
            );
            Ok::<String, AppError>(new_path)
        });
        cx.spawn(async move |this, cx| {
            let moved = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|result| result);
            let _ = this.update_in(cx, |view, window, cx| match moved {
                Ok(new_path) => {
                    for tab in &mut view.tabs {
                        if tab.path.as_deref() == Some(previous.as_str()) {
                            tab.path = Some(new_path.clone());
                        }
                    }
                    if view.timeline_for.as_deref() == Some(previous.as_str()) {
                        view.timeline_for = Some(new_path.clone());
                    }
                    view.workspace.update(cx, |model, cx| model.refresh(cx));
                    window.push_notification(tr!(cx, "请求已移动。", "Request moved."), cx);
                }
                Err(error) => {
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }
}
