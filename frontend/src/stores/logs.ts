import { defineStore } from 'pinia'
import { ref } from 'vue'
import { logsApi, type LogEntry, type LogsQueryParams } from '@/api/logs'

export const useLogsStore = defineStore('logs', () => {
  const logs = ref<LogEntry[]>([])
  const total = ref(0)
  const filesSearched = ref<string[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  const filters = ref<LogsQueryParams>({
    limit: 3, // 默认3条
  })

  async function fetchLogs(params?: LogsQueryParams) {
    loading.value = true
    error.value = null

    try {
      const queryParams = { ...filters.value, ...params }
      const response = await logsApi.queryLogs(queryParams)

      logs.value = response.logs
      total.value = response.total
      filesSearched.value = response.files_searched

      // Update filters with actual params used
      if (params) {
        filters.value = { ...filters.value, ...params }
      }
    } catch (err: any) {
      error.value = err.message || '获取日志失败'
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
      limit: 3,
    }
  }

  return {
    logs,
    total,
    filesSearched,
    loading,
    error,
    filters,
    fetchLogs,
    updateFilters,
    resetFilters,
  }
})
