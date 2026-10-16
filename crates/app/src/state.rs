//! 应用状态模型。
//!
//! 原来（Tauri + React）这一层是 Zustand 管 UI 状态、React Query 管服务端数据。
//! 迁移到 GPUI 后两者合一：`Entity<WorkspaceModel>` 持有工作区快照，
//! 视图通过 `cx.subscribe` 监听它的事件，变更即重绘；没有序列化边界。

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, EventEmitter, Task};
use thread_core::AppCore;
use thread_core::error::AppResult;
use thread_core::workspace::{WorkspaceIndex, WorkspaceSnapshot};

/// 工作区变化时广播给所有视图。
#[derive(Clone, Debug)]
pub enum WorkspaceEvent {
    /// 打开、刷新或关闭导致快照变化。
    Changed,
    /// 出现错误（打开失败、写入失败等）。
    Failed(String),
}

/// 打开中的工作区。所有视图共享同一个实体。
pub struct WorkspaceModel {
    pub core: Arc<AppCore>,
    pub snapshot: Option<WorkspaceSnapshot>,
    pub loading: bool,
    pub error: Option<String>,
    /// 外部文件变化后由 watcher 推送的一条提示。
    pub notice: Option<String>,
    _watch_task: Option<Task<()>>,
}

impl WorkspaceModel {
    pub fn new(core: Arc<AppCore>, cx: &mut Context<Self>) -> Self {
        let mut model = Self {
            core,
            snapshot: None,
            loading: false,
            error: None,
            notice: None,
            _watch_task: None,
        };
        model.reopen(cx);
        model
    }

    /// 恢复上次退出时打开的工作区。
    fn reopen(&mut self, cx: &mut Context<Self>) {
        if self.core.reopen_last_workspace().is_some() {
            self.snapshot = self.core.workspace.snapshot().ok();
            cx.emit(WorkspaceEvent::Changed);
        }
    }

    pub fn is_open(&self) -> bool {
        self.snapshot.is_some()
    }

    pub fn index(&self) -> Option<&WorkspaceIndex> {
        self.snapshot.as_ref().map(|snapshot| &snapshot.index)
    }

    pub fn root(&self) -> Option<PathBuf> {
        self.core.workspace.root_if_open()
    }

    /// 打开或创建工作区。`mode` 为 `create` 时先初始化目录结构。
    pub fn open(
        &mut self,
        path: PathBuf,
        name: Option<String>,
        create: bool,
        cx: &mut Context<Self>,
    ) {
        self.loading = true;
        self.error = None;
        cx.emit(WorkspaceEvent::Changed);
        cx.notify();

        let name = name.unwrap_or_else(|| {
            path.file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "Workspace".to_string())
        });

        let result = if create {
            self.core.create_workspace(&path, &name)
        } else {
            self.core.open_workspace(&path)
        };

        self.loading = false;
        match result {
            Ok(snapshot) => {
                tracing::info!(
                    path = %path.display(),
                    create,
                    requests = snapshot.index.request_count,
                    "workspace opened"
                );
                self.snapshot = Some(snapshot);
                self.notice = None;
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), create, %error, "workspace open failed");
                self.error = Some(error.to_string());
                cx.emit(WorkspaceEvent::Failed(error.to_string()));
            }
        }
        cx.emit(WorkspaceEvent::Changed);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        tracing::info!("workspace closed");
        self.core.workspace.close();
        self.snapshot = None;
        self.notice = None;
        cx.emit(WorkspaceEvent::Changed);
        cx.notify();
    }

    /// 重新扫描工作区（外部修改 / Git 切换分支后）。
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        match self.core.workspace.refresh() {
            Ok(_) => {
                self.snapshot = self.core.workspace.snapshot().ok();
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error.to_string());
            }
        }
        cx.emit(WorkspaceEvent::Changed);
        cx.notify();
    }

    /// 在当前工作区执行一个写操作，成功后自动刷新索引。
    ///
    /// 工作区操作大多是文件 IO，放在后台线程执行，避免卡顿渲染。
    pub fn run<F>(&mut self, cx: &mut Context<Self>, job: F)
    where
        F: FnOnce(PathBuf) -> AppResult<String> + Send + 'static,
    {
        let Some(root) = self.root() else {
            return;
        };
        let core = self.core.clone();
        cx.spawn(async move |this, cx| {
            let outcome: AppResult<String> = cx.background_spawn(async move { job(root) }).await;
            let _ = core.workspace.refresh();
            this.update(cx, |model, cx| {
                model.snapshot = model.core.workspace.snapshot().ok();
                match outcome {
                    Ok(message) => model.notice = Some(message),
                    Err(error) => {
                        model.error = Some(error.to_string());
                        cx.emit(WorkspaceEvent::Failed(error.to_string()));
                    }
                }
                cx.emit(WorkspaceEvent::Changed);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 启动外部文件变化监听（每个窗口只需要一次）。
    ///
    /// core 的 watcher 通过回调通知；这里转成 channel，由一个轮询任务
    /// 转发到模型上：后台重扫文件，成功后更新快照并广播。
    pub fn start_watching(&mut self, cx: &mut Context<Self>) {
        if self._watch_task.is_some() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel::<()>();
        self.core.watch_workspace(move |_| {
            let _ = sender.send(());
        });
        self._watch_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                if receiver.try_recv().is_err() {
                    continue; // 没有外部变化
                }
                let result = this.update(cx, |model, cx| {
                    let core = model.core.clone();
                    let scan = cx.background_spawn(async move { core.workspace.refresh() });
                    cx.spawn(async move |this, cx| {
                        if scan.await.is_ok()
                            && this
                                .update(cx, |model, cx| {
                                    model.snapshot = model.core.workspace.snapshot().ok();
                                    model.notice = Some(
                                        crate::tr!(
                                            cx,
                                            "工作区文件已在外部修改，已重新加载。",
                                            "Workspace files changed on disk and were reloaded."
                                        )
                                        .to_string(),
                                    );
                                    tracing::info!("workspace reloaded after external change");
                                    cx.emit(WorkspaceEvent::Changed);
                                    cx.notify();
                                })
                                .is_ok()
                        {
                            // 更新已广播。
                        }
                    })
                    .detach();
                });
                if result.is_err() {
                    break; // 窗口已关闭
                }
            }
        }));
    }
}

impl EventEmitter<WorkspaceEvent> for WorkspaceModel {}

/// 侧边栏 / 主区域切换的视图。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MainView {
    #[default]
    Request,
    History,
    Proxy,
    Certificates,
    Environments,
}

impl MainView {
    pub const ALL: [MainView; 5] = [
        MainView::Request,
        MainView::Environments,
        MainView::History,
        MainView::Proxy,
        MainView::Certificates,
    ];

    pub fn label(self, cx: &gpui_kit::App) -> &'static str {
        match self {
            Self::Request => crate::tr!(cx, "请求", "Requests"),
            Self::History => crate::tr!(cx, "历史", "History"),
            Self::Proxy => crate::tr!(cx, "代理", "Proxy"),
            Self::Certificates => crate::tr!(cx, "证书", "Certificates"),
            Self::Environments => crate::tr!(cx, "环境", "Environments"),
        }
    }
}
