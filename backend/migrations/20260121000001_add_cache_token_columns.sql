-- 添加缓存 token 列到 requests 表
--
-- 这些字段用于追踪 Anthropic API 的提示词缓存使用情况:
-- - cache_creation_input_tokens: 首次请求时创建缓存的 token 数
-- - cache_read_input_tokens: 后续请求从缓存读取的 token 数
--
-- 注意: 这些列已经在之前手动添加,本迁移文件仅用于记录

-- 空操作: 验证列已存在
SELECT cache_creation_input_tokens, cache_read_input_tokens FROM requests LIMIT 0;
