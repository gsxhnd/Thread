-- Thread / 灰线 — 阶段 3：抓包代理本地数据
--
-- 代理流量属于本地运行数据（见 docs/03.database.md §3.4），不进入工作区与 Git。
-- 仅在用户开启「持久化」时写入；未开启时流量只保留在内存中，应用退出即丢弃。
-- 大 Body 不入库，以文件形式保存在 `<数据目录>/proxy/bodies/`。

-- 代理会话：一次启动到停止之间的捕获周期
CREATE TABLE IF NOT EXISTS proxy_sessions (
    id              TEXT PRIMARY KEY,
    workspace_path  TEXT,                       -- 启动时打开的工作区；无工作区时为 NULL
    port            INTEGER NOT NULL,
    started_at      INTEGER NOT NULL,
    stopped_at      INTEGER
);

-- 代理条目：单次请求/响应（或一条 CONNECT 隧道）的摘要与头部
CREATE TABLE IF NOT EXISTS proxy_entries (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL REFERENCES proxy_sessions(id) ON DELETE CASCADE,
    captured_at         INTEGER NOT NULL,
    method              TEXT NOT NULL,
    url                 TEXT NOT NULL,          -- 已脱敏（敏感查询参数值替换为 ***）
    scheme              TEXT NOT NULL,
    host                TEXT NOT NULL,
    port                INTEGER NOT NULL,
    path                TEXT NOT NULL,
    status_code         INTEGER,
    status_text         TEXT,
    content_type        TEXT,
    mode                TEXT NOT NULL,          -- http | mitm | tunnel
    decrypted           INTEGER NOT NULL DEFAULT 0,
    error               TEXT,
    note                TEXT,                   -- 例如「未解密：隧道转发」
    request_headers     TEXT NOT NULL DEFAULT '[]',   -- JSON: [{key, value}]，已脱敏
    response_headers    TEXT NOT NULL DEFAULT '[]',   -- JSON: [{key, value}]，已脱敏
    request_body_size   INTEGER NOT NULL DEFAULT 0,   -- 实际字节数（可能大于落盘的截断内容）
    response_body_size  INTEGER NOT NULL DEFAULT 0,
    request_body_path   TEXT,
    response_body_path  TEXT,
    duration_ms         INTEGER NOT NULL DEFAULT 0,
    ttfb_ms             INTEGER
);

CREATE INDEX IF NOT EXISTS idx_proxy_entries_session  ON proxy_entries(session_id);
CREATE INDEX IF NOT EXISTS idx_proxy_entries_host     ON proxy_entries(host);
CREATE INDEX IF NOT EXISTS idx_proxy_entries_captured ON proxy_entries(captured_at DESC);
CREATE INDEX IF NOT EXISTS idx_proxy_sessions_started ON proxy_sessions(started_at DESC);
