<template>
  <div class="chart-panel">
    <div class="panel-header">
      <h3 class="panel-title">TOKEN USAGE BY API KEY</h3>
      <div class="panel-meta">
        <span class="meta-label">HOURLY</span>
      </div>
    </div>

    <div class="chart-container">
      <canvas ref="chartRef" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, onMounted } from 'vue'
import { dashboardApi, type TimeseriesResponse, type TimeseriesDataPoint } from '@/api/dashboard'
import { subDays, format, addDays } from 'date-fns'

const chartRef = ref<HTMLCanvasElement>()
let chartInstance: any = null

const responseData = ref<TimeseriesResponse | null>(null)
const isLoading = ref(false)

async function fetchData() {
  isLoading.value = true
  try {
    responseData.value = await dashboardApi.getTimeseriesTokens({
      start_date: format(subDays(new Date(), 1), 'yyyy-MM-dd'),
      end_date: format(addDays(new Date(), 1), 'yyyy-MM-dd'),
      group_by: 'api_key',
      interval: 'hour',
    })
  } catch (error) {
    console.error('Failed to fetch token usage by API key:', error)
  } finally {
    isLoading.value = false
  }
}

watch(responseData, (newData) => {
  if (newData && chartRef.value) {
    updateChart(newData.data)
  }
}, { deep: true })

function updateChart(data: TimeseriesDataPoint[]) {
  if (!chartRef.value) return

  if (!chartInstance) {
    createChart()
  }

  if (!chartInstance) return

  // Group data by API key
  const datasets = groupDataByApiKey(data)

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

function groupDataByApiKey(data: TimeseriesDataPoint[]) {
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

  // Extract timestamps from data
  data.forEach(point => {
    // Convert UTC to local time for display
    const localTimestamp = convertUTCToLocal(point.timestamp)
    timestamps.add(localTimestamp)
  })

  // Ensure we have all hours from the latest data point to the next hour
  const sortedTimestamps = Array.from(timestamps).sort()
  if (sortedTimestamps.length > 0) {
    const latest = sortedTimestamps[sortedTimestamps.length - 1]
    // Parse the latest timestamp and add the next hour
    const match = latest?.match(/(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2})/)
    if (match) {
      const date = match[1]!
      const hour = match[2]!
      const minute = match[3] || '00'
      const hourNum = parseInt(hour, 10)
      const nextHourNum = (hourNum + 1) % 24
      const nextHourStr = String(nextHourNum).padStart(2, '0')
      timestamps.add(`${date}T${nextHourStr}:${minute}`)
    }
  }

  return Array.from(timestamps).sort()
}

function getColor(index: number, alpha: number = 1): string {
  const colors = [
    `rgba(0, 255, 65, ${alpha})`,
    `rgba(0, 217, 255, ${alpha})`,
    `rgba(255, 107, 0, ${alpha})`,
    `rgba(255, 0, 65, ${alpha})`,
    `rgba(147, 51, 234, ${alpha})`,
    `rgba(255, 215, 0, ${alpha})`,
  ]
  return colors[index % colors.length]!
}

function createChart() {
  if (!chartRef.value) return

  const ctx = chartRef.value.getContext('2d')
  if (!ctx) return

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
      responsive: true,
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
            maxTicksLimit: 12,
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

onMounted(async () => {
  await nextTick()

  const tryCreateChart = () => {
    if (chartRef.value && !chartInstance) {
      createChart()
      if (responseData.value?.data) {
        updateChart(responseData.value.data)
      }
    } else if (!chartInstance) {
      requestAnimationFrame(tryCreateChart)
    }
  }

  tryCreateChart()
  fetchData()
})

defineExpose({
  refresh: fetchData
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

.panel-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.meta-label {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: #444;
}

.indicator-dot {
  width: 6px;
  height: 6px;
  background: #00ff41;
}

.indicator-dot.pulse {
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% {
    opacity: 1;
    box-shadow: 0 0 4px #00ff41;
  }
  50% {
    opacity: 0.6;
    box-shadow: 0 0 8px #00ff41;
  }
}

.chart-container {
  padding: 1.5rem;
  height: 280px;
}
</style>
