<template>
  <div class="chart-panel">
    <div class="panel-header">
      <h3 class="panel-title">TOKEN USAGE TREND</h3>
      <div class="panel-controls">
        <select v-model="groupBy" class="control-select" @change="fetchData">
          <option value="provider">By Provider</option>
          <option value="model">By Model</option>
          <option value="api_key">By API Key</option>
          <option value="instance">By Instance</option>
        </select>
        <select v-model="interval" class="control-select" @change="fetchData">
          <option value="day">Daily</option>
          <option value="hour">Hourly</option>
        </select>
      </div>
    </div>

    <div class="chart-container">
      <canvas ref="chartRef" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'
import { dashboardApi, type TimeseriesDataPoint } from '@/api/dashboard'
import { subDays, format } from 'date-fns'

const chartRef = ref<HTMLCanvasElement>()
let chartInstance: any = null
let resizeObserver: ResizeObserver | null = null

const groupBy = ref<'provider' | 'model' | 'api_key' | 'instance'>('provider')
const interval = ref<'hour' | 'day'>('day')
const loading = ref(false)
const error = ref('')

async function fetchData() {
  loading.value = true
  error.value = ''

  const endDate = format(new Date(), 'yyyy-MM-dd')
  const startDate = format(subDays(new Date(), 7), 'yyyy-MM-dd') // Last 7 days

  try {
    const response = await dashboardApi.getTimeseriesTokens({
      start_date: startDate,
      end_date: endDate,
      group_by: groupBy.value,
      interval: interval.value,
    })

    updateChart(response.data)
  } catch (err: any) {
    console.error('Failed to fetch timeseries data:', err)
    error.value = 'Failed to load data'
  } finally {
    loading.value = false
  }
}

function updateChart(data: TimeseriesDataPoint[]) {
  if (!chartInstance || !chartRef.value) return

  // Group data by label
  const datasets = groupDataByLabel(data)

  // Extract unique timestamps
  const timestamps = extractUniqueTimestamps(data)

  chartInstance.data.labels = timestamps
  chartInstance.data.datasets = datasets.map((dataset, index) => ({
    label: dataset.label,
    data: dataset.data,
    borderColor: getColor(index),
    backgroundColor: getColor(index, 0.1),
    borderWidth: 2,
    tension: 0.3,
    fill: true,
    pointRadius: 0,
    pointHoverRadius: 4,
  }))

  chartInstance.update()
}

function groupDataByLabel(data: TimeseriesDataPoint[]) {
  const groups = new Map<string, number[]>()

  data.forEach(point => {
    if (!groups.has(point.label)) {
      groups.set(point.label, [])
    }
    groups.get(point.label)!.push(point.value.tokens)
  })

  return Array.from(groups.entries()).map(([label, data]) => ({ label, data }))
}

// Convert UTC timestamp to local time for display
function convertUTCToLocal(utcTimestamp: string): string {
  // Parse the timestamp (format: "2026-01-19T13:00:00")
  // The timestamp is already in local time from backend, just parse and format it
  const date = new Date(utcTimestamp)

  // Format as local time
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  const hours = String(date.getHours()).padStart(2, '0')
  const minutes = String(date.getMinutes()).padStart(2, '0')

  return `${year}-${month}-${day}T${hours}:${minutes}` as string
}

function extractUniqueTimestamps(data: TimeseriesDataPoint[]): string[] {
  const timestamps = new Set<string>()
  data.forEach(point => {
    // Convert UTC to local time for display
    const localTimestamp = convertUTCToLocal(point.timestamp)
    timestamps.add(localTimestamp)
  })
  return Array.from(timestamps).sort()
}

function getColor(index: number, alpha: number = 1): string {
  const colors = [
    `rgba(0, 255, 65, ${alpha})`,      // Green
    `rgba(0, 217, 255, ${alpha})`,     // Blue
    `rgba(255, 107, 0, ${alpha})`,      // Orange
    `rgba(255, 0, 65, ${alpha})`,       // Red
    `rgba(147, 51, 234, ${alpha})`,     // Purple
    `rgba(255, 215, 0, ${alpha})`,     // Yellow
  ]
  return colors[index % colors.length]!
}

function createChart() {
  if (!chartRef.value) return

  const container = chartRef.value.parentElement
  if (!container) return

  // 如果已存在图表实例，先销毁
  if (chartInstance) {
    chartInstance.destroy()
    chartInstance = null
  }

  const ctx = chartRef.value.getContext('2d')
  if (!ctx) return

  // 使用全局的 window.Chart
  const Chart = (window as any).Chart

  Chart.defaults.color = '#666'
  Chart.defaults.font.family = '"IBM Plex Sans", monospace'

  chartInstance = new Chart(ctx, {
    type: 'line',
    data: {
      labels: [],
      datasets: [],
    },
    options: {
      responsive: true,  // 重新启用 responsive
      maintainAspectRatio: false,
      interaction: {
        mode: 'index',
        intersect: false,
      },
      plugins: {
        legend: {
          display: true,
          position: 'top',
          align: 'end',
          labels: {
            color: '#999',
            font: {
              size: 10,
              family: '"IBM Plex Sans", monospace',
            },
            boxWidth: 12,
            padding: 10,
          },
        },
        tooltip: {
          backgroundColor: '#1a1a1a',
          titleColor: '#e0e0e0',
          bodyColor: '#999',
          borderColor: '#333',
          borderWidth: 1,
          padding: 12,
          titleFont: {
            family: '"JetBrains Mono", monospace',
            size: 11,
            weight: '600',
          },
          bodyFont: {
            family: '"IBM Plex Sans", monospace',
            size: 10,
          },
          callbacks: {
            label: (context: any) => {
              const value = context.parsed.y
              return `${formatNumber(value)} tokens`
            },
          },
        },
      },
      scales: {
        x: {
          grid: {
            color: '#1a1a1a',
            drawBorder: false,
          },
          ticks: {
            font: {
              family: '"IBM Plex Sans", monospace',
              size: 9,
            },
            color: '#555',
            maxRotation: 0,
            autoSkip: true,
            maxTicksLimit: 10,
          },
        },
        y: {
          grid: {
            color: '#1a1a1a',
            drawBorder: false,
          },
          ticks: {
            font: {
              family: '"JetBrains Mono", monospace',
              size: 10,
            },
            color: '#555',
            callback: (value: any) => formatNumber(value as number),
          },
        },
      },
    },
  })
}

function formatNumber(num: number): string {
  if (num >= 1000000) {
    return `${(num / 1000000).toFixed(1)}M`
  } else if (num >= 1000) {
    return `${(num / 1000).toFixed(1)}K`
  }
  return num.toString()
}

let refreshInterval: number

onMounted(async () => {
  // 等待 DOM 完全渲染并确保 ref 已绑定
  await nextTick()

  // 如果 ref 还没有绑定，等待一帧后再试
  const tryCreateChart = () => {
    if (chartRef.value && !chartInstance) {
      createChart()
      fetchData()
    } else if (!chartInstance) {
      // ref 还没有绑定，使用 requestAnimationFrame 再试
      requestAnimationFrame(tryCreateChart)
    }
  }

  tryCreateChart()

  // Refresh every 30 seconds
  refreshInterval = window.setInterval(() => {
    fetchData()
  }, 30000)
})

onUnmounted(() => {
  if (refreshInterval) {
    clearInterval(refreshInterval)
  }

  // 清理 ResizeObserver
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }

  if (chartInstance) {
    chartInstance.destroy()
    chartInstance = null
  }
})
</script>

<style scoped>
.chart-panel {
  background: #0a0a0a;
  border: 1px solid #333;
  margin-bottom: 1rem;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem 1.5rem;
  border-bottom: 1px solid #1a1a1a;
}

.panel-title {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.65rem;
  font-weight: 600;
  letter-spacing: 0.15em;
  color: #666;
  margin: 0;
}

.panel-controls {
  display: flex;
  gap: 0.5rem;
}

.control-select {
  background: #1a1a1a;
  border: 1px solid #333;
  color: #999;
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.6rem;
  padding: 0.25rem 0.5rem;
  cursor: pointer;
  transition: border-color 0.2s;
}

.control-select:hover {
  border-color: #444;
}

.control-select:focus {
  outline: none;
  border-color: #00ff41;
}

.panel-loading,
.panel-error {
  padding: 3rem;
  display: flex;
  align-items: center;
  justify-content: center;
}

.loading-text,
.error-text {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.75rem;
  color: #444;
  letter-spacing: 0.1em;
}

.error-text {
  color: #ff0041;
}

.chart-container {
  padding: 1.5rem;
  height: 300px;
}
</style>
