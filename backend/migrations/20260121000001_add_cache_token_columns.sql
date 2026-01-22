-- 添加缓存 token 列到 requests 表
--
-- 这些字段用于追踪 Anthropic API 的提示词缓存使用情况:
-- - cache_creation_input_tokens: 首次请求时创建缓存的 token 数
-- - cache_read_input_tokens: 后续请求从缓存读取的 token 数

-- Add cache_creation_input_tokens if not exists
ALTER TABLE requests ADD COLUMN cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0;

-- Add cache_read_input_tokens if not exists
ALTER TABLE requests ADD COLUMN cache_read_input_tokens INTEGER NOT NULL DEFAULT 0;
