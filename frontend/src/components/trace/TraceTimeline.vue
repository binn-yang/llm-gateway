<template>
  <el-card class="timeline-card glass-strong" shadow="hover">
    <template #header>
      <span class="text-white font-bold">Request Timeline</span>
    </template>

    <div v-if="loading" class="flex justify-center items-center h-64">
      <el-icon class="is-loading text-white text-4xl"><Loading /></el-icon>
    </div>

    <div v-else class="chart-container">
      <canvas ref="chartCanvas"></canvas>
    </div>
  </el-card>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { Chart, type ChartData, type ChartOptions } from 'chart.js/auto'
import { Loading } from '@element-plus/icons-vue'
import type { Span } from '@/api/traces'
import { getDefaultBarOptions, chartColors } from '@/composables/useChartConfig'

const props = defineProps<{
  spans: Span[]
  loading?: boolean
}>()

const chartCanvas = ref<HTMLCanvasElement>()
let chartInstance: Chart | null = null

function buildTimelineData(): ChartData | null {
  if (!props.spans || props.spans.length === 0) return null

  const startTime = Math.min(...props.spans.map((s) => new Date(s.start_time).getTime()))

  return {
    labels: props.spans.map((s) => s.name),
    datasets: [
      {
        label: 'Duration (ms)',
        data: props.spans.map((s) => ({
          x: [new Date(s.start_time).getTime() - startTime, s.duration_ms],
          y: s.name,
        })) as any,
        backgroundColor: props.spans.map((s) =>
          s.status === 'ok' ? chartColors.successFill : chartColors.dangerFill
        ),
        borderColor: props.spans.map((s) =>
          s.status === 'ok' ? chartColors.success : chartColors.danger
        ),
        borderWidth: 1,
        borderSkipped: false,
      },
    ],
  }
}

function updateChart() {
  if (!chartCanvas.value) return

  const ctx = chartCanvas.value.getContext('2d')
  if (!ctx) return

  const data = buildTimelineData()
  if (!data) return

  // @ts-ignore - Chart.js type compatibility issue
  const options: ChartOptions = {
    ...getDefaultBarOptions(),
    indexAxis: 'y' as const,
  }

  if (chartInstance) {
    chartInstance.data = data
    chartInstance.options = options
    chartInstance.update()
  } else {
    chartInstance = new Chart(ctx, {
      type: 'bar',
      data,
      options,
    })
  }
}

watch(
  () => props.spans,
  () => {
    updateChart()
  },
  { deep: true }
)

onMounted(() => {
  updateChart()
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
