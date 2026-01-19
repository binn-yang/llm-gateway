import apiClient from './client'

export interface Span {
  span_id: string
  parent_id?: string
  request_id: string
  name: string
  kind: string
  start_time: string
  duration_ms: number
  status: string
  attributes?: string
}

export interface Trace {
  request_id: string
  model: string
  provider: string
  start_time: string
  duration_ms: number
  status: string
  input_tokens?: number
  output_tokens?: number
  spans: Span[]
}

export interface TracesQueryParams {
  limit?: number
  since_seconds?: number
  model?: string
  provider?: string
  status?: string
}

export interface TracesResponse {
  traces: Trace[]
  total: number
}

export const tracesApi = {
  /**
   * Query traces with filters
   */
  queryTraces: async (params?: TracesQueryParams): Promise<TracesResponse> => {
    const response = await apiClient.get<TracesResponse>('/traces', {
      params,
    })
    return response.data
  },

  /**
   * Get trace by request ID
   */
  getTrace: async (requestId: string): Promise<Trace> => {
    const response = await apiClient.get<Trace>(`/traces/${requestId}`)
    return response.data
  },
}
