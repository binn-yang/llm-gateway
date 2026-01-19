import { defineStore } from 'pinia'
import { ref } from 'vue'
import { tracesApi, type Trace, type TracesQueryParams } from '@/api/traces'

export const useTracesStore = defineStore('traces', () => {
  const traces = ref<Trace[]>([])
  const total = ref(0)
  const loading = ref(false)
  const error = ref<Error | null>(null)

  const filters = ref<TracesQueryParams>({
    limit: 50,
    since_seconds: 3600,
  })

  async function fetchTraces(params?: TracesQueryParams) {
    loading.value = true
    error.value = null

    try {
      const queryParams = { ...filters.value, ...params }
      const response = await tracesApi.queryTraces(queryParams)

      traces.value = response.traces
      total.value = response.total

      if (params) {
        filters.value = { ...filters.value, ...params }
      }
    } catch (err) {
      error.value = err as Error
      console.error('Failed to fetch traces:', err)
    } finally {
      loading.value = false
    }
  }

  async function fetchTrace(requestId: string): Promise<Trace> {
    loading.value = true
    error.value = null

    try {
      const trace = await tracesApi.getTrace(requestId)
      return trace
    } catch (err) {
      error.value = err as Error
      console.error('Failed to fetch trace:', err)
      throw err
    } finally {
      loading.value = false
    }
  }

  function updateFilters(newFilters: Partial<TracesQueryParams>) {
    filters.value = { ...filters.value, ...newFilters }
  }

  return {
    traces,
    total,
    loading,
    error,
    filters,
    fetchTraces,
    fetchTrace,
    updateFilters,
  }
})
