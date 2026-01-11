-- LLM Gateway Observability Schema
-- Phase 1: Logs, Spans, Metrics Snapshots, Retention Policy

-- 日志表 (支持全文检索和时间范围查询)
CREATE TABLE IF NOT EXISTS logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,          -- Unix毫秒时间戳
    level TEXT NOT NULL,                 -- ERROR/WARN/INFO/DEBUG/TRACE
    target TEXT NOT NULL,                -- Rust 模块路径
    message TEXT NOT NULL,

    -- 关联字段
    request_id TEXT,                     -- UUID (关键!)
    span_id TEXT,                        -- 当前 span ID (可选,用于关联到 spans 表)

    -- 上下文 (JSON灵活存储)
    fields TEXT                          -- {"api_key_name":"xxx","provider":"anthropic",...}

    -- Note: No FK constraint on span_id to allow logs without spans
    -- and to avoid insert order dependencies
);

CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON logs(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_logs_level ON logs(level);
CREATE INDEX IF NOT EXISTS idx_logs_request_id ON logs(request_id);

-- Spans 表 (轻量级追踪,非完整 OpenTelemetry)
CREATE TABLE IF NOT EXISTS spans (
    span_id TEXT PRIMARY KEY,
    parent_span_id TEXT,                 -- 支持嵌套 span
    request_id TEXT NOT NULL,

    name TEXT NOT NULL,                  -- "chat_completions", "load_balancer::select"
    kind TEXT NOT NULL,                  -- "server"/"client"/"internal"

    start_time INTEGER NOT NULL,         -- Unix 毫秒
    end_time INTEGER,                    -- NULL = 仍在运行
    duration_ms INTEGER,                 -- 计算值

    status TEXT,                         -- "ok"/"error"
    attributes TEXT,                     -- JSON: {provider, model, instance, ...}

    FOREIGN KEY (parent_span_id) REFERENCES spans(span_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_spans_request_id ON spans(request_id);
CREATE INDEX IF NOT EXISTS idx_spans_start_time ON spans(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_spans_duration ON spans(duration_ms DESC);

-- Metrics 快照表 (每5分钟一次)
CREATE TABLE IF NOT EXISTS metrics_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,

    -- 聚合指标 (JSON格式,灵活扩展)
    metrics TEXT NOT NULL,

    UNIQUE(timestamp)
);

CREATE INDEX IF NOT EXISTS idx_metrics_timestamp ON metrics_snapshots(timestamp DESC);

-- 数据保留策略配置
CREATE TABLE IF NOT EXISTS retention_policy (
    table_name TEXT PRIMARY KEY,
    ttl_days INTEGER NOT NULL,
    last_cleanup INTEGER NOT NULL        -- Unix 时间戳
);

-- 插入默认保留策略 (仅在表为空时)
INSERT OR IGNORE INTO retention_policy (table_name, ttl_days, last_cleanup) VALUES
    ('logs', 7, 0),
    ('spans', 7, 0),
    ('metrics_snapshots', 30, 0);
