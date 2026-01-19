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
  group_by: string
  data: {
    llm_requests_total?: Record<string, number>
    llm_tokens_total?: Record<string, number>
    llm_request_duration_seconds_sum?: Record<string, number>
    llm_request_duration_seconds_count?: Record<string, number>
    llm_instance_health_status?: Record<string, number>
    llm_instance_requests_total?: Record<string, number>
  }
}

export interface DashboardSummary {
  api_key_count: number
  provider_count: number
  instance_count: number
  today_requests: number
  today_tokens: number
  total_requests: number
  total_tokens: number
  health_status: boolean
  timestamp: string
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

  /**
   * Get dashboard summary
   */
  getSummary: async (): Promise<DashboardSummary> => {
    const response = await apiClient.get<DashboardSummary>('/dashboard/summary')
    return response.data
  },

  /**
   * Get token usage time series
   */
  getTimeseriesTokens: async (params: TimeseriesQueryParams): Promise<TimeseriesResponse> => {
    const response = await apiClient.get<TimeseriesResponse>('/dashboard/timeseries/tokens', {
      params,
    })
    return response.data
  },

  /**
   * Get instance health time series
   */
  getTimeseriesHealth: async (params: HealthTimeseriesQueryParams): Promise<HealthTimeseriesResponse> => {
    const response = await apiClient.get<HealthTimeseriesResponse>('/dashboard/timeseries/health', {
      params,
    })
    return response.data
  },

  /**
   * Get all provider instances health status
   */
  getInstancesHealth: async (): Promise<InstancesHealthResponse> => {
    const response = await apiClient.get<InstancesHealthResponse>('/dashboard/instances-health')
    return response.data
  },
}

// ============================================================================
// Time-Series API Types
// ============================================================================

export interface TimeseriesQueryParams {
  start_date: string
  end_date?: string
  group_by: 'provider' | 'model' | 'api_key' | 'instance'
  interval?: 'hour' | 'day'
}

export interface TimeseriesResponse {
  start_date: string
  end_date: string
  group_by: string
  interval: string
  data: TimeseriesDataPoint[]
}

export interface TimeseriesDataPoint {
  label: string
  timestamp: string
  value: {
    tokens: number
    requests: number
  }
}

export interface HealthTimeseriesQueryParams {
  start_date: string
  end_date?: string
  instance?: string
}

export interface HealthTimeseriesResponse {
  start_date: string
  end_date: string
  data: HealthDataPoint[]
}

export interface HealthDataPoint {
  provider: string
  instance: string
  timestamp: string
  health_status: string
  failover_count: number
}

// ============================================================================
// Instance Health Monitoring API Types
// ============================================================================

export interface InstanceHealthDetail {
  provider: string
  instance: string
  is_healthy: boolean
  duration_secs: number
  downtime_last_24h_secs: number
}

export interface InstancesHealthResponse {
  timestamp: string
  instances: InstanceHealthDetail[]
}
