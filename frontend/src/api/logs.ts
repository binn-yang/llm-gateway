import apiClient from './client'

export interface LogEntry {
  timestamp: string
  level: string
  target: string
  message: string
  request_id?: string
  fields: string
}

export interface LogsQueryParams {
  limit?: number
  level?: string
  since_seconds?: number
  grep?: string
  request_id?: string
}

export interface LogsResponse {
  logs: LogEntry[]
  total: number
  limit: number
}

export const logsApi = {
  /**
   * Query logs with filters
   */
  queryLogs: async (params?: LogsQueryParams): Promise<LogsResponse> => {
    const response = await apiClient.get<LogsResponse>('/logs', {
      params,
    })
    return response.data
  },

  /**
   * Get logs for a specific request
   */
  getRequestLogs: async (requestId: string): Promise<LogsResponse> => {
    const response = await apiClient.get<LogsResponse>(`/logs`, {
      params: { request_id: requestId },
    })
    return response.data
  },
}
