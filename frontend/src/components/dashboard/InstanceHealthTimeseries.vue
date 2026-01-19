<template>
  <div class="chart-panel">
    <div class="panel-header">
      <h3 class="panel-title">INSTANCE HEALTH STATUS</h3>
      <div class="panel-controls">
        <select v-model="selectedInstance" class="control-select">
          <option value="">All Instances</option>
          <option v-for="instance in availableInstances" :key="instance" :value="instance">
            {{ instance }}
          </option>
        </select>
      </div>
    </div>

    <div class="chart-container">
      <canvas ref="chartRef" />
    </div>

    <!-- Health Statistics -->
    <div v-if="!loading && !error && healthStats" class="health-stats">
      <div class="stat-item">
        <span class="stat-label">TOTAL UPTIME</span>
        <span class="stat-value">{{ healthStats.uptime }}%</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">FAILOVERS</span>
        <span class="stat-value">{{ healthStats.failovers }}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">STATUS</span>
        <span class="stat-value" :class="healthStats.currentStatus.toLowerCase()">
          {{ healthStats.currentStatus }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { dashboardApi, type HealthTimeseriesResponse, type HealthDataPoint } from '@/api/dashboard'
import { subDays, format } from 'date-fns'

const chartRef = ref<HTMLCanvasElement>()
let chartInstance: any = null
let resizeObserver: ResizeObserver | null = null

const selectedInstance = ref('')
const loading = ref(false)
const error = ref('')
const healthData = ref<HealthDataPoint[]>([])

const availableInstances = computed(() => {
  const instances = new Set<string>()
  healthData.value.forEach(point => {
    instances.add(point.instance)
  })
  return Array.from(instances).sort()
})

const healthStats = computed(() => {
  if (healthData.value.length === 0) return null

  const totalPoints = healthData.value.length
  const healthyPoints = healthData.value.filter(p => p.health_status === 'healthy').length
  const totalFailovers = healthData.value.reduce((sum, p) => sum + p.failover_count, 0)
  const lastStatus = healthData.value[healthData.value.length - 1]?.health_status || 'unknown'

  return {
    uptime: totalPoints > 0 ? ((healthyPoints / totalPoints) * 100).toFixed(1) : '0.0',
    failovers: totalFailovers,
    currentStatus: lastStatus,
  }
})

async function fetchData() {
  loading.value = true
  error.value = ''

  const endDate = format(new Date(), 'yyyy-MM-dd')
  const startDate = format(subDays(new Date(), 7), 'yyyy-MM-dd') // Last 7 days

  try {
    const response = await dashboardApi.getTimeseriesHealth({
      start_date: startDate,
      end_date: endDate,
      instance: selectedInstance.value || undefined,
    })

    healthData.value = response.data
    updateChart(response.data)
  } catch (err: any) {
    console.error('Failed to fetch health data:', err)
    error.value = 'Failed to load data'
  } finally {
    loading.value = false
  }
}

function updateChart(data: HealthDataPoint[]) {
  if (!chartInstance || !chartRef.value) return

  // Group by instance
  const instances = groupDataByInstance(data)

  // Extract unique timestamps
  const timestamps = extractUniqueTimestamps(data)

  chartInstance.data.labels = timestamps
  chartInstance.data.datasets = instances.map((instance, index) => ({
    label: instance.name,
    data: instance.data,
    borderColor: getColor(index),
    backgroundColor: getColor(index, 0.1),
    borderWidth: 2,
    tension: 0.1,
    stepped: true,
    pointRadius: 0,
    pointHoverRadius: 4,
    spanGaps: true,
  }))

  chartInstance.update()
}

function groupDataByInstance(data: HealthDataPoint[]) {
  const instances = new Map<string, { name: string; data: number[] }>()

  data.forEach(point => {
    const key = `${point.provider}:${point.instance}`
    if (!instances.has(key)) {
      instances.set(key, {
        name: point.instance,
        data: [],
      })
    }
    // Convert status to numeric: healthy = 1, unhealthy = 0
    instances.get(key)!.data.push(point.health_status === 'healthy' ? 1 : 0)
  })

  return Array.from(instances.values())
}

function extractUniqueTimestamps(data: HealthDataPoint[]): string[] {
  const timestamps = new Set<string>()
  data.forEach(point => timestamps.add(point.timestamp))
  return Array.from(timestamps).sort()
}

function getColor(index: number, alpha: number = 1): string {
  const colors = [
    `rgba(0, 255, 65, ${alpha})`,      // Green
    `rgba(0, 217, 255, ${alpha})`,     // Blue
    `rgba(255, 107, 0, ${alpha})`,      // Orange
    `rgba(255, 0, 65, ${alpha})`,       // Red
  ]
  return colors[index % colors.length]
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
            label: (context) => {
              const value = context.parsed.y
              return value === 1 ? 'Healthy' : 'Unhealthy'
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
          min: 0,
          max: 1.2,
          ticks: {
            font: {
              family: '"IBM Plex Sans", monospace',
              size: 10,
            },
            color: '#555',
            stepSize: 1,
            callback: (value) => {
              if (value === 1) return 'Healthy'
              if (value === 0) return 'Unhealthy'
              return ''
            },
          },
        },
      },
    },
  })
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

watch(selectedInstance, () => {
  fetchData()
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
  height: 280px;
}

.health-stats {
  display: flex;
  justify-content: space-around;
  padding: 1rem 1.5rem;
  border-top: 1px solid #1a1a1a;
}

.stat-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.25rem;
}

.stat-label {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: #555;
}

.stat-value {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.85rem;
  font-weight: 700;
  color: #e0e0e0;
}

.stat-value.healthy {
  color: #00ff41;
}

.stat-value.unhealthy {
  color: #ff0041;
}
</style>
