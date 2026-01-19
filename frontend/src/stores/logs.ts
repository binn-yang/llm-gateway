import { defineStore } from 'pinia'
import { ref } from 'vue'
import { logsApi, type LogEntry, type LogsQueryParams } from '@/api/logs'

export const useLogsStore = defineStore('logs', () => {
  const logs = ref<LogEntry[]>([])
  const total = ref(0)
  const loading = ref(false)
  const error = ref<Error | null>(null)

  const filters = ref<LogsQueryParams>({
    limit: 100,
    level: undefined,
    since_seconds: 3600,
    grep: undefined,
  })

  async function fetchLogs(params?: LogsQueryParams) {
    loading.value = true
    error.value = null

    try {
      const queryParams = { ...filters.value, ...params }
      const response = await logsApi.queryLogs(queryParams)

      logs.value = response.logs
      total.value = response.total

      // Update filters with actual params used
      if (params) {
        filters.value = { ...filters.value, ...params }
      }
    } catch (err) {
      error.value = err as Error
      console.error('Failed to fetch logs:', err)
    } finally {
      loading.value = false
    }
  }

  function updateFilters(newFilters: Partial<LogsQueryParams>) {
    filters.value = { ...filters.value, ...newFilters }
  }

  function resetFilters() {
    filters.value = {
      limit: 100,
      level: undefined,
      since_seconds: 3600,
      grep: undefined,
    }
  }

  return {
    logs,
    total,
    loading,
    error,
    filters,
    fetchLogs,
    updateFilters,
    resetFilters,
  }
})
