<template>
  <div class="space-y-6">
    <!-- Metrics Cards -->
    <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
      <MetricsCard
        v-for="card in metricsCards"
        :key="card.key"
        :title="card.title"
        :value="card.value.value"
        :icon="card.icon"
        :color="card.color"
      />
    </div>

    <!-- Stats Section -->
    <div class="glass-strong rounded-xl p-6">
      <h2 class="text-xl font-bold text-white mb-4">Statistics</h2>

      <div v-if="statsLoading" class="text-center text-white">
        <el-icon class="is-loading"><Loading /></el-icon>
        <p>Loading...</p>
      </div>

      <div v-else-if="statsError" class="text-red-300">
        <p>Error loading stats</p>
      </div>

      <div
        v-else-if="statsData"
        class="grid grid-cols-1 md:grid-cols-2 gap-6"
      >
        <div>
          <h3 class="text-lg font-semibold text-white mb-2">Requests</h3>
          <p class="text-3xl font-bold text-white">
            {{ formatNumber(statsData.total_requests) }}
          </p>
        </div>

        <div>
          <h3 class="text-lg font-semibold text-white mb-2">Tokens</h3>
          <p class="text-3xl font-bold text-white">
            {{ formatNumber(statsData.total_tokens) }}
          </p>
        </div>

        <div>
          <h3 class="text-lg font-semibold text-white mb-2">Avg Latency</h3>
          <p class="text-3xl font-bold text-white">
            {{ formatLatency(statsData.avg_latency_seconds) }}
          </p>
        </div>

        <div>
          <h3 class="text-lg font-semibold text-white mb-2">Error Rate</h3>
          <p class="text-3xl font-bold text-white">
            {{ formatPercent(statsData.error_rate) }}
          </p>
        </div>
      </div>
    </div>

    <!-- Provider Stats -->
    <div v-if="statsData?.by_provider" class="glass-strong rounded-xl p-6">
      <h2 class="text-xl font-bold text-white mb-4">By Provider</h2>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div
          v-for="(stats, provider) in statsData.by_provider"
          :key="provider"
          class="bg-white/10 rounded-lg p-4"
        >
          <h3 class="text-lg font-semibold text-white capitalize mb-2">
            {{ provider }}
          </h3>
          <div class="space-y-2 text-white">
            <p>Requests: {{ formatNumber(stats.requests) }}</p>
            <p>Tokens: {{ formatNumber(stats.tokens) }}</p>
            <p>Latency: {{ formatLatency(stats.avg_latency_seconds) }}</p>
            <p>Error Rate: {{ formatPercent(stats.error_rate) }}</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useMetricsStore } from '@/stores/metrics'
import { usePolling } from '@/composables/usePolling'
import { dashboardApi, type StatsResponse } from '@/api/dashboard'
import MetricsCard from '@/components/dashboard/MetricsCard.vue'
import { Loading } from '@element-plus/icons-vue'

const metricsStore = useMetricsStore()

// Stats polling
const { data: statsData, isLoading: statsLoading, error: statsError } =
  usePolling<StatsResponse>({
    fn: () => dashboardApi.getStats({ group_by: 'provider' }),
    interval: 5000,
    autoStart: true,
  })

// Metrics cards configuration
interface MetricsCard {
  key: string
  title: string
  value: { value: string }
  icon: string
  color: 'primary' | 'secondary' | 'accent' | 'danger'
}

const requestsValue = ref('0')
const tokensValue = ref('0')
const latencyValue = ref('0s')
const errorsValue = ref('0%')

const metricsCards: MetricsCard[] = [
  {
    key: 'requests',
    title: 'Total Requests',
    value: requestsValue,
    icon: 'Document',
    color: 'primary',
  },
  {
    key: 'tokens',
    title: 'Total Tokens',
    value: tokensValue,
    icon: 'ChatDotRound',
    color: 'secondary',
  },
  {
    key: 'latency',
    title: 'Avg Latency',
    value: latencyValue,
    icon: 'Timer',
    color: 'accent',
  },
  {
    key: 'errors',
    title: 'Error Rate',
    value: errorsValue,
    icon: 'Warning',
    color: 'danger',
  },
]

// Update metrics from store
function updateMetrics() {
  const requests = metricsStore.getMetricValue('llm_requests_total')
  const tokens = metricsStore.getMetricValue('llm_tokens_total')

  requestsValue.value = formatNumber(requests)
  tokensValue.value = formatNumber(tokens)

  const data = statsData.value
  if (data) {
    latencyValue.value = formatLatency(data.avg_latency_seconds)
    errorsValue.value = formatPercent(data.error_rate)
  }
}

// Watch stats data changes and update metrics
function watchStats() {
  updateMetrics()
}

// Fetch metrics on mount and update periodically
onMounted(() => {
  metricsStore.fetchMetrics()
  updateMetrics()

  const interval = setInterval(() => {
    metricsStore.fetchMetrics()
    watchStats()
  }, 5000)

  onUnmounted(() => {
    clearInterval(interval)
  })
})

// Formatters
function formatNumber(num: number): string {
  if (num >= 1000000) {
    return `${(num / 1000000).toFixed(1)}M`
  } else if (num >= 1000) {
    return `${(num / 1000).toFixed(1)}K`
  }
  return num.toString()
}

function formatLatency(seconds: number): string {
  if (seconds < 1) {
    return `${(seconds * 1000).toFixed(0)}ms`
  }
  return `${seconds.toFixed(2)}s`
}

function formatPercent(rate: number): string {
  return `${(rate * 100).toFixed(2)}%`
}
</script>
