import apiClient from './client'

// 日志条目格式 - 匹配JSONL实际格式
export interface LogEntry {
  timestamp: string // ISO 8601格式
  level: string // INFO, WARN, ERROR, DEBUG
  target: string // 如 "llm_gateway::handlers::messages"
  message: string // 日志消息内容
  fields?: Record<string, any> // 额外字段（duration_ms等）

  // span对象包含请求上下文
  span?: {
    request_id?: string
    api_key_name?: string
    model?: string
    provider?: string
    instance?: string
    endpoint?: string
    name?: string
    [key: string]: string | undefined // 索引签名
  }

  // spans数组（分布式追踪）
  spans?: Array<{
    request_id?: string
    api_key_name?: string
    model?: string
    provider?: string
    instance?: string
    endpoint?: string
    name?: string
    [key: string]: string | undefined // 索引签名
  }>
}

// 查询参数 - 匹配后端LogsQueryParams
export interface LogsQueryParams {
  limit?: number // 默认3，最大1000
  request_id?: string // Trace查询
  grep?: string // 文本搜索
  date?: string // YYYY-MM-DD格式
  level?: string // 日志等级过滤（前端使用，后端未实现但不影响）
}

// 响应格式 - 匹配后端LogsResponse
export interface LogsResponse {
  logs: LogEntry[]
  total: number
  files_searched: string[] // 新增字段
}

export const logsApi = {
  /**
   * Query logs with filters
   */
  queryLogs: async (params?: LogsQueryParams): Promise<LogsResponse> => {
    const response = await apiClient.get<LogsResponse>('/dashboard/logs', {
      params,
    })
    return response.data
  },

  /**
   * Get logs for a specific request (trace query)
   */
  getRequestLogs: async (requestId: string): Promise<LogsResponse> => {
    const response = await apiClient.get<LogsResponse>('/dashboard/logs', {
      params: { request_id: requestId },
    })
    return response.data
  },
}
