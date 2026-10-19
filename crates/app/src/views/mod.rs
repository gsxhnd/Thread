//! 各主视图的渲染与交互，按视图拆分文件；处理器以 `impl AppView` 分布在各文件中。

pub mod certificates;
pub mod content;
pub mod environments;
pub mod history;
pub mod protocols;
pub mod proxy;
pub mod request;
pub mod response;
pub mod settings;
pub mod shortcuts;
pub mod sidebar;
pub mod statusbar;
pub mod url_vars;
pub mod welcome;
