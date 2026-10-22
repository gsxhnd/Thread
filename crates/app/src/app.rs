//! 根视图：标题栏 + 侧边栏 + 内容区。
//!
//! 所有视图共享同一个 `AppView` 实体：状态直接挂在字段上，事件直接用
//! `cx.listener`，没有跨实体路由。渲染函数按视图拆在 `views/` 下，
//! 处理器以 `impl AppView` 的形式分布在各视图文件里。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{EditorState, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::select::SelectState;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, TitleBar, WindowExt as _, h_flex, h_resizable, resizable_panel,
    v_flex,
};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, Global, IntoElement as _,
    ParentElement as _, PathPromptOptions, Render, Styled as _, Subscription, Task, WeakEntity,
    Window, div, px,
};
use thread_core::AppCore;
use thread_core::error::AppError;
use thread_core::services::http::{HttpRequestConfig, HttpResponse};
use thread_core::workspace::index::RequestNode;

use crate::state::{MainView, WorkspaceEvent, WorkspaceModel};
use crate::theme;
use crate::theme::ThemePreference;
use crate::views;

/// 长连接日志里的一行。
#[derive(Clone, Debug)]
pub struct SessionLine {
    pub kind: String,
    pub text: String,
}

/// 还没写入历史库的一次发送，挂在临时请求的时间线上。
#[derive(Clone, Debug)]
pub struct LocalRun {
    pub id: String,
    pub at: i64,
    pub status: u16,
    pub duration_ms: i64,
    pub url: String,
    /// 发出去的请求。点这一行时和响应一起显示。
    pub request: Option<HttpRequestConfig>,
    /// 这次发送的响应。点时间线时把响应区切回这一条。
    pub response: Option<HttpResponse>,
}

/// 集合树里正在原位编辑的名称。回车提交，Esc 取消。
pub struct InlineRename {
    pub node_id: String,
    pub path: String,
    pub request: bool,
    pub input: Entity<InputState>,
    /// 输入框真正拿到焦点之后，失焦才提交。打开菜单的那一下失焦不算。
    pub armed: bool,
    /// 持有输入框订阅。字段本身不读，丢掉这次重命名时订阅一起取消。
    #[expect(dead_code)]
    pub(crate) watch: Subscription,
}

/// 一个打开的请求标签页（保存的请求或临时请求）。
pub struct RequestTab {
    /// 标签页自己的 id。关闭按钮用它，不用下标，避免关一个时把旁边的标签一起关掉。
    pub id: String,
    /// 编辑框的订阅。内容一变就标成未保存，关掉标签时一起释放。
    pub edit_watches: Vec<Subscription>,
    /// 工作区相对路径；`None` 表示尚未保存的临时请求。
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
    pub protocol: views::request::Protocol,
    pub method: Entity<SelectState<Vec<&'static str>>>,
    pub url: Entity<InputState>,
    /// 地址栏变量补全的订阅，随标签页一起释放。
    pub url_watch: Option<Subscription>,
    /// 临时请求的时间线。保存进工作区之后改走历史库。
    pub local_runs: Vec<LocalRun>,
    /// 时间线上选中的记录。点一行同时看到那次的请求和响应。
    pub timeline_selected: Option<String>,
    /// 选中那次发送的请求。只在时间线里展示，不改正在编辑的表单。
    pub timeline_request: Option<HttpRequestConfig>,
    /// 这条展示用的是已经替换过变量、真正发出去的内容。认证已经进了请求头。
    pub timeline_wire: bool,
    pub params: Vec<views::request::KvRow>,
    pub headers: Vec<views::request::KvRow>,
    /// 表单请求体，和查询参数分开存。文件字段带本地路径。
    pub form_fields: Vec<views::request::KvRow>,
    /// `inherit` / `none` / `bearer` / `basic` / `apiKey`。
    pub auth_kind: String,
    pub auth_token: Entity<InputState>,
    pub auth_username: Entity<InputState>,
    pub auth_password: Entity<InputState>,
    pub auth_key: Entity<InputState>,
    pub auth_value: Entity<InputState>,
    /// API 密钥放在 `header` 或 `query`。
    pub auth_in: String,
    pub body_kind: Entity<SelectState<Vec<&'static str>>>,
    pub body: Entity<EditorState>,
    /// GraphQL variables，或其它协议暂未使用的第二块编辑器。
    pub variables: Entity<EditorState>,
    pub service: Entity<InputState>,
    pub rpc_method: Entity<InputState>,
    pub operation_name: Entity<InputState>,
    pub message: Entity<InputState>,
    pub section: views::request::RequestSection,
    pub pane: views::request::EditorPane,
    pub response_section: views::response::ResponseSection,
    /// 响应体格式/预览模式，按标签页分别记忆。
    pub response_body_mode: views::response::ResponseBodyMode,
    /// 发送中的任务 id，用于取消。
    pub task_id: Option<String>,
    pub sending: bool,
    pub response: Option<HttpResponse>,
    pub error: Option<String>,
    pub started_at: Option<i64>,
    pub tls: bool,
    pub auto_reconnect: bool,
    pub binary_message: bool,
    pub pool_id: Option<String>,
    pub services: Vec<thread_core::services::grpc::ServiceInfo>,
    /// WebSocket / GraphQL subscription / gRPC stream 的活动 id。
    pub connection_id: Option<String>,
    pub log: Vec<SessionLine>,
    pub schema: Option<thread_core::services::graphql::GraphqlSchema>,
    pub suggestions: Vec<thread_core::services::graphql::FieldCompletion>,
    pub ws_protocols: Vec<String>,
    pub saved_messages: Vec<thread_core::workspace::formats::WsSavedMessage>,
    pub proto_source: String,
    pub proto_file_path: Option<String>,
    pub timeout_ms: u64,
    /// MQTT 等只读文档，保存时原样写回。
    pub preserved: Option<serde_json::Value>,
}

/// 代理页轮询与展示状态。
pub struct ProxyPane {
    pub status: thread_core::services::proxy::ProxyStatus,
    pub port: Entity<InputState>,
    pub settings: thread_core::services::proxy::CaptureSettings,
    pub entries: Vec<thread_core::services::proxy::ProxyEntrySummary>,
    pub selected: Option<thread_core::services::proxy::ProxyEntryDetail>,
    pub filter_host: Entity<InputState>,
    pub filter_method: Entity<InputState>,
    pub filter_status: Entity<InputState>,
    pub max_entries: Entity<InputState>,
    pub max_body_kb: Entity<InputState>,
    /// 代理停掉之后 `status.session_id` 会被清掉，清空流量仍要用它。
    pub last_session_id: Option<String>,
}

/// 历史页状态。
pub struct HistoryPane {
    pub items: Vec<thread_core::services::history::HistorySummary>,
    pub total: i64,
    pub selected: Option<thread_core::services::history::HistoryDetail>,
    pub body: Option<String>,
    pub loading: bool,
}

/// 环境页状态。
pub struct EnvironmentPane {
    pub selected_path: Option<String>,
    pub detail: Option<thread_core::workspace::EnvironmentDetail>,
}

/// 证书页状态。
pub struct CertificatePane {
    pub info: Option<thread_core::services::proxy::ca::CaInfo>,
    pub error: Option<String>,
}

pub struct AppView {
    pub core: Arc<AppCore>,
    pub workspace: Entity<WorkspaceModel>,
    pub view: MainView,
    /// 侧边栏点开的集合。有值时主区域显示集合页，而不是请求标签。
    pub open_collection: Option<String>,
    /// 侧边栏里收起的集合路径。点箭头切换，点文件夹本身打开集合页。
    pub collapsed_collections: HashSet<String>,
    pub theme: ThemePreference,
    pub sidebar_collapsed: bool,
    pub editor_layout: crate::theme::EditorLayout,
    /// 正在询问要不要关闭的标签。避免同一次点击弹出两次。
    closing_tab: Option<String>,
    /// 临时请求保存成功后要关掉的标签 id。
    pub(crate) close_after_save: Option<String>,
    pub tree: Entity<gpui_kit::component::tree::TreeState>,
    /// 侧边栏里正在原位重命名的节点。
    pub rename: Option<InlineRename>,
    pub tabs: Vec<RequestTab>,
    pub active_tab: Option<usize>,
    pub url_assist: views::url_vars::UrlAssist,
    /// 同一次点击里只关一个标签。
    tab_close_armed: bool,
    /// 当前请求时间线（历史库里属于这个文件的记录）。
    pub timeline: Vec<thread_core::services::history::HistorySummary>,
    pub timeline_for: Option<String>,
    pub proxy: ProxyPane,
    pub history: HistoryPane,
    pub environment: EnvironmentPane,
    pub certificate: CertificatePane,
    /// 环境页的变量表单。
    pub env_key: Entity<InputState>,
    pub env_value: Entity<InputState>,
    /// 设置子窗口。关掉之后下一次打开会重新创建。
    pub settings_window: Option<AnyWindowHandle>,
    /// 设置窗口正在创建，避免同一次点击连开两个。
    pub(crate) settings_pending: bool,
    /// 上次刷新时的工作区根目录。只有根目录变了才清掉打开的标签页。
    workspace_root: Option<PathBuf>,
    pub _watch: Option<Task<()>>,
    pub _subscriptions: Vec<Subscription>,
}

/// 主窗口里的 `AppView`，供全局快捷键找回。
#[derive(Clone)]
pub struct MainApp(pub WeakEntity<AppView>);

impl Global for MainApp {}

impl AppView {
    pub fn new(core: Arc<AppCore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(|cx| WorkspaceModel::new(core.clone(), cx));
        workspace.update(cx, |model, cx| model.start_watching(cx));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &workspace,
            window,
            |this, _, event: &WorkspaceEvent, window, cx| {
                match event {
                    WorkspaceEvent::Changed => {
                        // 打开/刷新/关闭后重建各页数据。
                        this.reload_after_workspace_change(cx);
                        views::url_vars::resync_open_urls(this, window, cx);
                    }
                    WorkspaceEvent::Failed(message) => {
                        window.push_notification(
                            (
                                gpui_kit::component::notification::NotificationType::Error,
                                message.as_str(),
                            ),
                            cx,
                        );
                    }
                }
                cx.notify();
            },
        ));

        let capture = core.config.proxy_capture_settings();
        let port_entity = cx.new(|cx| {
            let port = core
                .workspace
                .config()
                .map(|config| config.proxy.port)
                .unwrap_or(9090);
            InputState::new(window, cx).default_value(port.to_string())
        });
        let filter_host =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "域名", "Host")));
        let filter_method =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "方法", "Method")));
        let filter_status =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "状态", "Status")));
        let max_entries =
            cx.new(|cx| InputState::new(window, cx).default_value(capture.max_entries.to_string()));
        let max_body_kb = cx.new(|cx| {
            InputState::new(window, cx).default_value((capture.max_body_bytes / 1024).to_string())
        });

        let tree = cx.new(|cx| gpui_kit::component::tree::TreeState::new(cx).items(Vec::new()));

        let env_key =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "变量名", "Variable")));
        let env_value =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "值", "Value")));
        let var_editor =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "值", "Value")));
        let workspace_root = workspace.read(cx).root();

        let theme_pref = theme::stored(&core.config);
        let sidebar_collapsed = theme::sidebar_collapsed(&core.config);
        let editor_layout = theme::editor_layout(&core.config);

        let mut view = Self {
            proxy: ProxyPane {
                status: thread_core::services::proxy::ProxyStatus::idle(),
                port: port_entity,
                settings: capture,
                entries: Vec::new(),
                selected: None,
                filter_host,
                filter_method,
                filter_status,
                max_entries,
                max_body_kb,
                last_session_id: None,
            },
            history: HistoryPane {
                items: Vec::new(),
                total: 0,
                selected: None,
                body: None,
                loading: false,
            },
            environment: EnvironmentPane {
                selected_path: None,
                detail: None,
            },
            certificate: CertificatePane {
                info: None,
                error: None,
            },
            env_key,
            env_value,
            settings_window: None,
            settings_pending: false,
            workspace_root,
            core,
            workspace,
            view: MainView::Request,
            open_collection: None,
            collapsed_collections: HashSet::new(),
            theme: theme_pref,
            sidebar_collapsed,
            editor_layout,
            closing_tab: None,
            close_after_save: None,
            tree,
            rename: None,
            tabs: Vec::new(),
            active_tab: None,
            url_assist: views::url_vars::UrlAssist::new(var_editor),
            tab_close_armed: false,
            timeline: Vec::new(),
            timeline_for: None,
            _watch: None,
            _subscriptions: subscriptions,
        };

        view.start_proxy_event_loop(cx);
        // `WorkspaceModel::new` 在订阅建立之前就发出了 Changed，那次事件被丢掉，
        // 树会一直空着，直到用户关掉再打开工作区。这里在订阅之后补一次重建。
        if view.workspace.read(cx).is_open() {
            views::sidebar::rebuild_tree(&mut view, cx);
            view.reload_history(cx);
        }

        for input in [
            view.proxy.filter_host.clone(),
            view.proxy.filter_method.clone(),
            view.proxy.filter_status.clone(),
        ] {
            view._subscriptions.push(cx.subscribe_in(
                &input,
                window,
                |_, _, event: &gpui_kit::component::input::InputEvent, _, cx| {
                    if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                        cx.notify();
                    }
                },
            ));
        }

        view
    }

    /// 工作区变化后刷新依赖它的页面数据。
    ///
    /// 集合的新建、重命名、删除也会广播 `Changed`。只有换了工作区根目录
    /// 才清掉标签页，否则右键操作会把正在编辑的请求关掉。
    pub fn reload_after_workspace_change(&mut self, cx: &mut Context<Self>) {
        let root = self.workspace.read(cx).root();
        if root != self.workspace_root {
            self.dismiss_open_tabs();
            self.environment.selected_path = None;
            self.environment.detail = None;
            self.workspace_root = root;
        } else {
            self.sync_open_tabs(cx);
        }
        views::sidebar::rebuild_tree(self, cx);
        self.reload_history(cx);
    }

    fn dismiss_open_tabs(&mut self) {
        let task_ids: Vec<String> = self
            .tabs
            .iter()
            .filter_map(|tab| tab.task_id.clone())
            .collect();
        for task_id in &task_ids {
            self.core.cancel_http(task_id);
        }
        for index in 0..self.tabs.len() {
            self.release_live_session(index);
        }
        self.tabs.clear();
        self.active_tab = None;
    }

    /// 索引变了之后，丢掉已经不在工作区里的标签，并刷新还在的请求名。
    fn sync_open_tabs(&mut self, cx: &Context<Self>) {
        enum TabPlan {
            Keep,
            Rename(String),
            Drop,
        }

        let plan: Vec<TabPlan> = {
            let model = self.workspace.read(cx);
            let Some(index) = model.index() else {
                return;
            };
            self.tabs
                .iter()
                .map(|tab| match &tab.path {
                    None => TabPlan::Keep,
                    Some(path) => match index.find_request(path) {
                        Some(location) => TabPlan::Rename(location.request.name.clone()),
                        None => TabPlan::Drop,
                    },
                })
                .collect()
        };

        for (index, plan) in plan.into_iter().enumerate().rev() {
            match plan {
                TabPlan::Drop => {
                    if let Some(task_id) = self.tabs[index].task_id.as_deref() {
                        self.core.cancel_http(task_id);
                    }
                    self.release_live_session(index);
                    self.tabs.remove(index);
                    self.active_tab = match self.active_tab {
                        Some(active) if active == index => {
                            if self.tabs.is_empty() {
                                None
                            } else {
                                Some(active.saturating_sub(1).min(self.tabs.len() - 1))
                            }
                        }
                        Some(active) if active > index => Some(active - 1),
                        other => other,
                    };
                }
                TabPlan::Rename(name) => self.tabs[index].name = name,
                TabPlan::Keep => {}
            }
        }
    }

    /// 原生目录选择框 → 打开已存在的工作区。
    pub fn prompt_open_workspace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr!(cx, "选择工作区文件夹", "Choose a workspace folder").into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                this.update(cx, |view, cx| {
                    view.open_workspace_path(path, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn open_workspace_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.workspace.update(cx, |model, cx| {
            model.open(path, None, false, cx);
        });
    }

    /// 新建工作区：选一个空目录（或让 init 建目录）。
    pub fn prompt_create_workspace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                tr!(
                    cx,
                    "选择新工作区的文件夹",
                    "Choose a folder for the new workspace"
                )
                .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                this.update(cx, |view, cx| {
                    let name = path
                        .file_name()
                        .map(|value| value.to_string_lossy().to_string())
                        .unwrap_or_else(|| tr!(cx, "新工作区", "New workspace").to_string());
                    view.workspace.update(cx, |model, cx| {
                        model.open(path, Some(name), true, cx);
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn close_workspace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |model, cx| model.close(cx));
    }

    /// 侧边栏里点了一个集合：主区域换成这个集合的页面。
    pub fn open_collection(&mut self, path: String, cx: &mut Context<Self>) {
        self.view = crate::state::MainView::Request;
        self.open_collection = Some(path);
        cx.notify();
    }

    /// 侧边栏里点了一个请求：已打开则聚焦，未打开则新建标签页。
    pub fn open_request(
        &mut self,
        node: &RequestNode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(existing) = self
            .tabs
            .iter()
            .position(|tab| tab.path.as_deref() == Some(&node.path))
        {
            // 标签已经开着时也要回到请求页。停在环境页时再点同一个请求，
            // 只改 active_tab 会让主区域继续停在环境页。
            self.open_collection = None;
            self.view = crate::state::MainView::Request;
            self.active_tab = Some(existing);
            cx.notify();
            return;
        }
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let path = node.path.clone();
        let read_path = path.clone();
        let history_path = path.clone();
        let workspace_key = root.to_string_lossy().into_owned();
        let db = self.core.db.clone();
        let core = self.core.clone();
        let receiver = core.spawn(async move {
            let loaded = thread_core::workspace::ops::read_request(&root, &read_path);
            let latest = thread_core::services::history::latest_for_request(
                &db,
                &workspace_key,
                &history_path,
            )
            .ok()
            .flatten();
            (loaded, latest)
        });
        let task = cx.spawn(async move |this, cx| {
            let (loaded, latest) = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .unwrap_or_else(|error| (Err(error), None));
            this.update_in(cx, |view, window, cx| {
                let mut tab = RequestTab::from_file(path, loaded, window, cx);
                if let Some(detail) = latest.as_ref() {
                    tab.restore_last_run(detail, window, cx);
                }
                view.open_collection = None;
                view.view = crate::state::MainView::Request;
                view.tabs.push(tab);
                view.active_tab = Some(view.tabs.len() - 1);
                if let Some(url) = view.tabs.last().map(|tab| tab.url.clone()) {
                    views::url_vars::watch_request_url(view, url, window, cx);
                }
                let index = view.tabs.len() - 1;
                views::request::watch_tab_edits(view, index, window, cx);
                cx.notify();
            })
            .ok();
        });
        task.detach();
    }

    /// 新建临时 HTTP 请求标签页。
    pub fn new_draft_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_draft_tab_for(views::request::Protocol::Http, window, cx);
    }

    /// 新建指定协议的临时请求，并切到请求视图。
    pub fn new_draft_tab_for(
        &mut self,
        protocol: views::request::Protocol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_collection = None;
        self.view = crate::state::MainView::Request;
        self.tabs.push(RequestTab::draft(protocol, window, cx));
        self.active_tab = Some(self.tabs.len() - 1);
        if let Some(url) = self.tabs.last().map(|tab| tab.url.clone()) {
            views::url_vars::watch_request_url(self, url, window, cx);
        }
        let index = self.tabs.len() - 1;
        views::request::watch_tab_edits(self, index, window, cx);
        cx.notify();
    }

    pub fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab_close_armed || index >= self.tabs.len() || self.closing_tab.is_some() {
            return;
        }
        let tab_id = self.tabs[index].id.clone();
        let dirty = self.tabs[index].dirty;
        let has_path = self.tabs[index].path.is_some();
        if !dirty {
            self.finish_close_tab(index, cx);
            return;
        }
        self.closing_tab = Some(tab_id.clone());
        let name = self.tabs[index].name.clone();
        let weak = cx.entity().downgrade();
        let title = tr!(
            cx,
            format!("“{name}”还有未保存的修改"),
            format!("“{name}” has unsaved changes")
        );
        let detail = tr!(
            cx,
            "保存后再关闭，或者放弃这些修改。",
            "Save before closing, or discard the changes."
        );
        let save_label = tr!(cx, "保存并关闭", "Save and close");
        let discard_label = tr!(cx, "不保存", "Don't save");
        let cancel_label = tr!(cx, "取消", "Cancel");
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak_escape = weak.clone();
            let weak_cancel = weak.clone();
            let weak_discard = weak.clone();
            let weak_save = weak.clone();
            let discard_id = tab_id.clone();
            let save_id = tab_id.clone();
            alert
                .title(title.to_string())
                .description(detail)
                .w(px(420.))
                .on_cancel(move |_, _, cx| {
                    let _ = weak_escape.update(cx, |view, cx| {
                        view.closing_tab = None;
                        cx.notify();
                    });
                    true
                })
                .footer(
                    DialogFooter::new()
                        .child(Button::new("unsaved-cancel").label(cancel_label).on_click(
                            move |_, window, cx| {
                                let _ = weak_cancel.update(cx, |view, cx| {
                                    view.closing_tab = None;
                                    cx.notify();
                                });
                                window.close_dialog(cx);
                            },
                        ))
                        .child(
                            Button::new("unsaved-discard")
                                .label(discard_label)
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let _ = weak_discard.update(cx, |view, cx| {
                                        view.closing_tab = None;
                                        let Some(index) =
                                            view.tabs.iter().position(|tab| tab.id == discard_id)
                                        else {
                                            return;
                                        };
                                        view.finish_close_tab(index, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("unsaved-save")
                                .primary()
                                .label(save_label)
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let _ = weak_save.update(cx, |view, cx| {
                                        view.closing_tab = None;
                                        let Some(index) =
                                            view.tabs.iter().position(|tab| tab.id == save_id)
                                        else {
                                            return;
                                        };
                                        if has_path {
                                            view.save_request(index, window, cx);
                                            view.finish_close_tab(index, cx);
                                        } else {
                                            view.close_after_save = Some(save_id.clone());
                                            view.save_request_as(index, window, cx);
                                        }
                                    });
                                }),
                        ),
                )
        });
    }

    pub(crate) fn finish_close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.tab_close_armed || index >= self.tabs.len() {
            return;
        }
        self.tab_close_armed = true;
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |view, _| {
                view.tab_close_armed = false;
            });
        })
        .detach();
        if let Some(task_id) = self.tabs[index].task_id.clone() {
            self.core.cancel_http(&task_id);
        }
        self.release_live_session(index);
        self.tabs.remove(index);
        self.active_tab = match self.active_tab {
            Some(active) if active == index => {
                if self.tabs.is_empty() {
                    None
                } else {
                    Some(active.saturating_sub(1))
                }
            }
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
        cx.notify();
    }

    /// Ctrl/Cmd+S：保存当前请求。临时请求会写成工作区里的新文件。
    pub fn save_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view != crate::state::MainView::Request {
            return;
        }
        let Some(index) = self.active_tab else {
            return;
        };
        if self
            .tabs
            .get(index)
            .and_then(|tab| tab.path.as_ref())
            .is_some()
        {
            self.save_request(index, window, cx);
        } else if index < self.tabs.len() {
            self.save_request_as(index, window, cx);
        }
    }

    pub fn set_editor_layout(
        &mut self,
        layout: crate::theme::EditorLayout,
        cx: &mut Context<Self>,
    ) {
        self.editor_layout = layout;
        theme::store_editor_layout(&self.core.config, layout);
        cx.notify();
    }

    pub fn toggle_editor_layout(&mut self, cx: &mut Context<Self>) {
        let next = match self.editor_layout {
            theme::EditorLayout::Stacked => theme::EditorLayout::Columns,
            theme::EditorLayout::Columns => theme::EditorLayout::Stacked,
        };
        self.set_editor_layout(next, cx);
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        theme::store_sidebar_collapsed(&self.core.config, self.sidebar_collapsed);
        cx.notify();
    }

    /// 在已打开的请求标签之间循环。`delta` 为 1 去下一个，-1 去上一个。
    pub fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let len = self.tabs.len() as isize;
        let current = self.active_tab.unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(len) as usize;
        self.open_collection = None;
        self.view = crate::state::MainView::Request;
        self.active_tab = Some(next);
        cx.notify();
    }

    /// 关掉标签页时释放还活着的长连接。
    fn release_live_session(&self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let Some(id) = tab.connection_id.as_deref() else {
            return;
        };
        match tab.protocol {
            views::request::Protocol::Websocket => {
                self.core.websockets.disconnect(id);
            }
            views::request::Protocol::Graphql => {
                self.core.graphql.unsubscribe(id);
            }
            views::request::Protocol::Grpc => {
                self.core.protos.cancel_stream(id);
            }
            views::request::Protocol::Http | views::request::Protocol::Mqtt => {}
        }
    }

    /// 把 WebSocket / 订阅 / gRPC 流事件写进对应标签页。
    pub fn apply_live_event(&mut self, event: thread_core::LiveEvent) {
        match event {
            thread_core::LiveEvent::WsMessage(message) => {
                self.push_session_log(&message.connection_id, &message.direction, &message.payload);
            }
            thread_core::LiveEvent::WsStatus(status) => {
                let detail = status.reason.unwrap_or_default();
                let connection_id = status.connection_id.clone();
                let kind = status.status.clone();
                self.push_session_log(&connection_id, &kind, &detail);
                if kind == "open" {
                    self.set_session_idle(&connection_id);
                }
                if kind == "closed" || kind == "error" {
                    self.clear_session(&connection_id);
                }
            }
            thread_core::LiveEvent::Graphql(event) => {
                let text = event
                    .data
                    .as_ref()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                self.push_session_log(&event.subscription_id, &event.event, &text);
                if event.event == "complete" || event.event == "error" {
                    self.clear_session(&event.subscription_id);
                }
            }
            thread_core::LiveEvent::Grpc(event) => {
                let text = event.error.clone().unwrap_or_else(|| {
                    event
                        .data
                        .as_ref()
                        .map(|value| {
                            serde_json::to_string_pretty(value)
                                .unwrap_or_else(|_| value.to_string())
                        })
                        .unwrap_or_default()
                });
                if !text.is_empty() {
                    self.push_session_log(&event.stream_id, &event.direction, &text);
                }
                if event.done || event.error.is_some() {
                    self.clear_session(&event.stream_id);
                }
            }
        }
    }

    fn push_session_log(&mut self, connection_id: &str, kind: &str, text: &str) {
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.connection_id.as_deref() == Some(connection_id))
        else {
            return;
        };
        tab.log.push(SessionLine {
            kind: kind.to_string(),
            text: text.to_string(),
        });
        if tab.log.len() > 400 {
            let excess = tab.log.len() - 400;
            tab.log.drain(0..excess);
        }
    }

    fn set_session_idle(&mut self, connection_id: &str) {
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.connection_id.as_deref() == Some(connection_id))
        {
            tab.sending = false;
        }
    }

    fn clear_session(&mut self, connection_id: &str) {
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.connection_id.as_deref() == Some(connection_id))
        {
            tab.connection_id = None;
            tab.sending = false;
        }
    }

    /// 轮询代理引擎事件，更新代理页状态。
    fn start_proxy_event_loop(&mut self, cx: &mut Context<Self>) {
        if self._watch.is_some() {
            return;
        }
        let core = self.core.clone();
        self._watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(400))
                    .await;

                let events = core.take_proxy_events();
                let live = core.take_live_events();
                let mut status: Option<thread_core::services::proxy::ProxyStatus> = None;
                let mut entries: Vec<thread_core::services::proxy::ProxyEntrySummary> = Vec::new();
                for event in events {
                    match event {
                        thread_core::ProxyEvent::Entry(entry) => entries.push(entry),
                        thread_core::ProxyEvent::Status(current) => status = Some(current),
                    }
                }
                if status.is_none() && entries.is_empty() && live.is_empty() {
                    continue;
                }
                let outcome = this.update(cx, |view, cx| {
                    for event in live {
                        view.apply_live_event(event);
                    }
                    if let Some(status) = status {
                        if let Some(session_id) = status.session_id.clone() {
                            view.proxy.last_session_id = Some(session_id);
                        }
                        view.proxy.status = status;
                    }
                    if !entries.is_empty() {
                        view.proxy.entries.extend(entries);
                        if view.proxy.entries.len() > 2000 {
                            let excess = view.proxy.entries.len() - 2000;
                            view.proxy.entries.drain(0..excess);
                        }
                    }
                    cx.notify();
                });
                if outcome.is_err() {
                    break;
                }
            }
        }));
    }

    /// 切换视图（侧边栏导航）。
    pub fn switch_view(&mut self, view: MainView, _: &mut Window, cx: &mut Context<Self>) {
        self.open_collection = None;
        self.view = view;
        match view {
            MainView::History => self.reload_history(cx),
            MainView::Certificates => self.reload_certificate(cx),
            MainView::Environments => self.reload_environment(cx),
            MainView::Proxy => {
                // 每次进入刷新一次状态，流量靠事件循环持续推进。
                self.refresh_proxy_status(cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn set_theme(
        &mut self,
        preference: ThemePreference,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.theme = preference;
        theme::set(&self.core.config, preference, cx);
        cx.notify();
    }

    pub fn set_locale(
        &mut self,
        locale: crate::i18n::Locale,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::i18n::set(&self.core.config, locale, cx);
        crate::install_menus(cx);
        self.refresh_placeholders(window, cx);
        cx.notify();
    }

    fn refresh_placeholders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = tr!(cx, "域名", "Host").to_string();
        let method = tr!(cx, "方法", "Method").to_string();
        let status = tr!(cx, "状态", "Status").to_string();
        let variable = tr!(cx, "变量名", "Variable").to_string();
        let value = tr!(cx, "值", "Value").to_string();
        let name = tr!(cx, "名称", "Name").to_string();
        let operation = tr!(cx, "操作名，可空", "Operation name, optional").to_string();
        let message = tr!(cx, "要发送的消息", "Message to send").to_string();

        let fields = [
            (self.proxy.filter_host.clone(), host),
            (self.proxy.filter_method.clone(), method),
            (self.proxy.filter_status.clone(), status),
            (self.env_key.clone(), variable),
            (self.env_value.clone(), value.clone()),
        ];
        for (input, text) in fields {
            input.update(cx, |state, cx| state.set_placeholder(text, window, cx));
        }

        let mut tabs = Vec::new();
        for tab in &self.tabs {
            let mut rows = Vec::new();
            for row in tab
                .params
                .iter()
                .chain(tab.headers.iter())
                .chain(tab.form_fields.iter())
            {
                rows.push((row.key.clone(), row.value.clone()));
            }
            tabs.push((tab.operation_name.clone(), tab.message.clone(), rows));
        }
        for (operation_name, message_input, rows) in tabs {
            let operation = operation.clone();
            let message = message.clone();
            operation_name.update(cx, |state, cx| state.set_placeholder(operation, window, cx));
            message_input.update(cx, |state, cx| state.set_placeholder(message, window, cx));
            for (key, field_value) in rows {
                let name = name.clone();
                let value = value.clone();
                key.update(cx, |state, cx| state.set_placeholder(name, window, cx));
                field_value.update(cx, |state, cx| state.set_placeholder(value, window, cx));
            }
        }
    }

    /// 顶部标题栏右侧：布局切换和设置。关闭工作区在左侧的工作区菜单里。
    fn titlebar_trailing(&mut self, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        let open = self.workspace.read(cx).is_open();
        let layout_icon = match self.editor_layout {
            theme::EditorLayout::Stacked => IconName::Rows2,
            theme::EditorLayout::Columns => IconName::Columns2,
        };
        let mut buttons = Vec::new();
        if open {
            buttons.push(
                Button::new("editor-layout")
                    .ghost()
                    .icon(layout_icon)
                    .tooltip_with_action(
                        match self.editor_layout {
                            theme::EditorLayout::Stacked => tr!(cx, "上下布局", "Stacked layout"),
                            theme::EditorLayout::Columns => {
                                tr!(cx, "左右布局", "Side by side layout")
                            }
                        },
                        &crate::ToggleLayout,
                        None,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_editor_layout(cx);
                    }))
                    .into_any_element(),
            );
        }
        buttons.push(
            Button::new("open-settings")
                .ghost()
                .icon(IconName::Settings)
                .tooltip_with_action(tr!(cx, "设置", "Settings"), &crate::OpenSettings, None)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_settings(window, cx);
                }))
                .into_any_element(),
        );
        buttons
    }
}

fn workspace_switcher(
    this: &AppView,
    workspace_name: Option<String>,
    cx: &mut Context<AppView>,
) -> impl gpui_kit::IntoElement {
    let weak = cx.entity().downgrade();
    let recents = this.core.config.session().recent_workspaces;
    let current = this
        .workspace
        .read(cx)
        .root()
        .map(|path| path.to_string_lossy().into_owned());
    let label =
        workspace_name.unwrap_or_else(|| tr!(cx, "选择工作区", "Choose workspace").to_string());

    Button::new("workspace-switch")
        .ghost()
        .small()
        .label(label)
        .dropdown_menu(move |menu, _, cx| {
            let mut menu = menu;
            if recents.is_empty() {
                menu = menu.item(PopupMenuItem::new(tr!(
                    cx,
                    "还没有最近的工作区",
                    "No recent workspaces"
                )));
            }
            for recent in &recents {
                let path = std::path::PathBuf::from(&recent.path);
                let weak = weak.clone();
                let selected = current.as_deref() == Some(recent.path.as_str());
                let name = if selected {
                    format!("✓ {}", recent.name)
                } else {
                    recent.name.clone()
                };
                menu = menu.item(PopupMenuItem::new(name).on_click(move |_, _, cx| {
                    let path = path.clone();
                    let _ = weak.update(cx, |view, cx| view.open_workspace_path(path, cx));
                }));
            }
            let open_weak = weak.clone();
            let create_weak = weak.clone();
            let close_weak = weak.clone();
            let workspace_open = current.is_some();
            let mut menu = menu
                .separator()
                .item(
                    PopupMenuItem::new(tr!(cx, "打开工作区…", "Open workspace…")).on_click(
                        move |_, window, cx| {
                            let _ = open_weak.update_in(cx, |view, window, cx| {
                                view.prompt_open_workspace(window, cx);
                            });
                            let _ = window;
                        },
                    ),
                )
                .item(
                    PopupMenuItem::new(tr!(cx, "新建工作区…", "New workspace…")).on_click(
                        move |_, window, cx| {
                            let _ = create_weak.update_in(cx, |view, window, cx| {
                                view.prompt_create_workspace(window, cx);
                            });
                            let _ = window;
                        },
                    ),
                );
            if workspace_open {
                menu = menu.separator().item(
                    PopupMenuItem::new(tr!(cx, "关闭工作区", "Close workspace")).on_click(
                        move |_, window, cx| {
                            let _ = close_weak.update_in(cx, |view, window, cx| {
                                view.close_workspace(window, cx);
                            });
                            let _ = window;
                        },
                    ),
                );
            }
            menu
        })
}

impl Render for AppView {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        let theme = cx.theme().clone();
        let workspace_name = self
            .workspace
            .read(cx)
            .index()
            .map(|index| index.name.clone());

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                TitleBar::new()
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Button::new("sidebar-toggle")
                                    .ghost()
                                    .icon(if self.sidebar_collapsed {
                                        IconName::PanelLeftOpen
                                    } else {
                                        IconName::PanelLeftClose
                                    })
                                    .tooltip_with_action(
                                        if self.sidebar_collapsed {
                                            tr!(cx, "展开侧边栏", "Show sidebar")
                                        } else {
                                            tr!(cx, "收起侧边栏", "Hide sidebar")
                                        },
                                        &crate::ToggleSidebar,
                                        None,
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_sidebar(cx);
                                    })),
                            )
                            .child(workspace_switcher(self, workspace_name, cx)),
                    )
                    .child(h_flex().gap_1().children(self.titlebar_trailing(cx))),
            )
            .child(div().flex_1().min_h_0().w_full().overflow_hidden().child(
                if self.workspace.read(cx).is_open() {
                    div()
                        .size_full()
                        .child(
                            h_resizable("app-sidebar")
                                .child(
                                    resizable_panel()
                                        .visible(!self.sidebar_collapsed)
                                        .size(px(240.))
                                        .size_range(px(180.)..px(480.))
                                        .child(
                                            views::sidebar::render(self, window, cx)
                                                .into_any_element(),
                                        ),
                                )
                                .child(
                                    resizable_panel()
                                        .child(views::content::render(self, window, cx)),
                                ),
                        )
                        .into_any_element()
                } else {
                    views::welcome::render(self, window, cx).into_any_element()
                },
            ))
            .child(views::statusbar::render(self, cx))
    }
}
