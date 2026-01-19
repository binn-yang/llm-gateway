import { defineStore } from 'pinia'
import { ref } from 'vue'
import { dashboardApi } from '@/api/dashboard'

export interface MetricData {
  name: string
  value: number
  help?: string
}

export interface MetricsResponse {
  timestamp: string
  metrics: string
}

export const useMetricsStore = defineStore('metrics', () => {
  const rawMetrics = ref<string>('')
  const metrics = ref<Map<string, MetricData>>(new Map())
  const isLoading = ref(false)
  const error = ref<Error | null>(null)
  const lastFetchTime = ref<Date | null>(null)

  async function fetchMetrics() {
    isLoading.value = true
    error.value = null
    try {
      const response = await dashboardApi.getMetrics()
      rawMetrics.value = response.metrics
      lastFetchTime.value = new Date()

      // Parse Prometheus metrics format
      const parsed = parsePrometheusMetrics(response.metrics)
      metrics.value = parsed
    } catch (e) {
      error.value = e as Error
      console.error('Failed to fetch metrics:', e)
    } finally {
      isLoading.value = false
    }
  }

  function getMetric(name: string): MetricData | undefined {
    return metrics.value.get(name)
  }

  function getMetricValue(name: string): number {
    return metrics.value.get(name)?.value ?? 0
  }

  return {
    rawMetrics,
    metrics,
    isLoading,
    error,
    lastFetchTime,
    fetchMetrics,
    getMetric,
    getMetricValue,
  }
})

/**
 * Parse Prometheus metrics text format into structured data
 */
function parsePrometheusMetrics(text: string): Map<string, MetricData> {
  const result = new Map<string, MetricData>()
  const lines = text.split('\n')

  for (const line of lines) {
    // Skip comments and empty lines
    if (line.startsWith('#') || !line.trim()) {
      continue
    }

    // Parse metric line: metric_name{labels} value
    const match = line.match(/^(\w+(?:\{[^}]*\})?)\s+(\d+\.?\d*)/)
    if (match && match[1] && match[2]) {
      const fullName = match[1]
      const value = parseFloat(match[2])

      // Extract metric name (without labels)
      const name = fullName.replace(/\{.*/, '')
      if (!name) continue

      // Aggregate metrics by base name
      const existing = result.get(name)
      if (existing) {
        existing.value += value
      } else {
        result.set(name, { name, value })
      }
    }
  }

  return result
}
