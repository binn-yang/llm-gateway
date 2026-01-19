<template>
  <el-card class="chart-card glass-strong" shadow="hover">
    <template #header>
      <div class="flex justify-between items-center">
        <span class="text-white font-bold">Requests Over Time</span>
        <el-radio-group v-model="timeRange" size="small" @change="handleTimeRangeChange">
          <el-radio-button label="1h">1H</el-radio-button>
          <el-radio-button label="6h">6H</el-radio-button>
          <el-radio-button label="24h">24H</el-radio-button>
        </el-radio-group>
      </div>
    </template>

    <div v-if="loading" class="flex justify-center items-center h-64">
      <el-icon class="is-loading text-white text-4xl"><Loading /></el-icon>
    </div>

    <div v-else-if="error" class="flex justify-center items-center h-64">
      <el-alert type="error" :closable="false">
        Failed to load requests data
      </el-alert>
    </div>

    <div v-else class="chart-container">
      <canvas ref="chartCanvas"></canvas>
    </div>
  </el-card>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { Chart, type ChartData, type ChartOptions } from 'chart.js/auto'
import { Loading } from '@element-plus/icons-vue'
import {
  getDefaultLineOptions,
  createGradient,
  chartColors,
} from '@/composables/useChartConfig'

interface DataPoint {
  timestamp: string
  requests: number
}

const props = defineProps<{
  provider?: string
}>()

const chartCanvas = ref<HTMLCanvasElement>()
const timeRange = ref<'1h' | '6h' | '24h'>('1h')
const loading = ref(false)
const error = ref(false)
const chartData = ref<DataPoint[]>([])

let chartInstance: Chart | null = null

// Fetch data based on time range
async function fetchData() {
  loading.value = true
  error.value = false

  try {
    const sinceSeconds =
      timeRange.value === '1h' ? 3600 : timeRange.value === '6h' ? 21600 : 86400

    // For now, generate mock data
    // TODO: Replace with actual API call when backend supports time-series data
    const now = Date.now()
    const interval = 60000 // 1 minute
    const points = Math.floor((sinceSeconds * 1000) / interval)

    chartData.value = Array.from({ length: points }, (_, i) => ({
      timestamp: new Date(now - (points - i) * interval).toISOString(),
      requests: Math.floor(Math.random() * 100) + 50,
    }))

    updateChart()
  } catch (err) {
    console.error('Failed to fetch requests data:', err)
    error.value = true
  } finally {
    loading.value = false
  }
}

// Initialize or update chart
function updateChart() {
  if (!chartCanvas.value) return

  const ctx = chartCanvas.value.getContext('2d')
  if (!ctx) return

  // Create gradient
  const gradient = createGradient(ctx, chartColors.primaryFill, 'rgba(102, 126, 234, 0.0)')

  const data: ChartData = {
    labels: chartData.value.map((p) => new Date(p.timestamp).toLocaleTimeString()),
    datasets: [
      {
        label: 'Requests',
        data: chartData.value.map((p) => p.requests),
        borderColor: chartColors.primary,
        backgroundColor: gradient,
        fill: true,
        tension: 0.4,
        pointRadius: 0,
        pointHoverRadius: 6,
      },
    ],
  }

  const options: ChartOptions = {
    ...getDefaultLineOptions(),
    plugins: {
      ...getDefaultLineOptions().plugins,
      title: {
        display: false,
      },
    },
  }

  if (chartInstance) {
    chartInstance.data = data
    chartInstance.options = options
    chartInstance.update()
  } else {
    chartInstance = new Chart(ctx, {
      type: 'line',
      data,
      options,
    })
  }
}

function handleTimeRangeChange() {
  fetchData()
}

onMounted(() => {
  fetchData()
})

onUnmounted(() => {
  if (chartInstance) {
    chartInstance.destroy()
    chartInstance = null
  }
})
</script>

<style scoped>
.chart-container {
  position: relative;
  height: 300px;
  width: 100%;
}
</style>
