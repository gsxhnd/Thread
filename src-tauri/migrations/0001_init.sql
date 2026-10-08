-- Thread / 灰线 — 本地数据层初始 schema
--
-- 该库只保存「派生索引」与「本地运行数据」（见 docs/03.database.md）：
-- 请求历史、响应体文件索引等。工作区文件才是集合/请求/环境的真相来源。
-- 代理流量、Cookie、证书等表在其对应阶段（阶段 3 及以后）以新的迁移文件追加。

-- 请求历史：已发送请求与响应快照
CREATE TABLE IF NOT EXISTS history_entries (
    id                 TEXT PRIMARY KEY,
    workspace_path     TEXT NOT NULL,
    request_file_path  TEXT,                    -- 相对工作区的请求文件路径；临时请求为 NULL
    protocol           TEXT NOT NULL,
    name               TEXT,
    method             TEXT,
    url                TEXT,
    request_snapshot   TEXT NOT NULL,           -- JSON: 发送时的请求配置
    response_snapshot  TEXT,                    -- JSON: 状态/Headers/计时/大小（不含大 Body）
    response_body_path TEXT,                    -- 大响应体落盘路径
    status_code        INTEGER,
    duration_ms        INTEGER,
    created_at         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_history_workspace ON history_entries(workspace_path);
CREATE INDEX IF NOT EXISTS idx_history_created   ON history_entries(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_history_protocol  ON history_entries(protocol);
