import apiClient from './client'

export interface MetricsResponse {
  timestamp: string
  metrics: string
}

export interface StatsQueryParams {
  group_by?: string
  since_seconds?: number
}

export interface StatsResponse {
  timestamp: string
  total_requests: number
  total_tokens: number
  avg_latency_seconds: number
  error_rate: number
  by_provider?: Record<string, ProviderStats>
}

export interface ProviderStats {
  requests: number
  tokens: number
  avg_latency_seconds: number
  error_rate: number
}

export interface ConfigSummary {
  server: {
    host: string
    port: number
    log_level: string
  }
  routing: {
    default_provider?: string
  }
  providers: {
    openai: number
    anthropic: number
    gemini: number
  }
}

export const dashboardApi = {
  /**
   * Get Prometheus metrics
   */
  getMetrics: async (): Promise<MetricsResponse> => {
    const response = await apiClient.get<MetricsResponse>('/dashboard/metrics')
    return response.data
  },

  /**
   * Get aggregated statistics
   */
  getStats: async (params?: StatsQueryParams): Promise<StatsResponse> => {
    const response = await apiClient.get<StatsResponse>('/dashboard/stats', {
      params,
    })
    return response.data
  },

  /**
   * Get configuration summary (with secrets masked)
   */
  getConfig: async (): Promise<ConfigSummary> => {
    const response = await apiClient.get<ConfigSummary>('/dashboard/config')
    return response.data
  },
}
