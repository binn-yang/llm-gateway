<template>
  <div class="chart-panel">
    <div class="panel-header">
      <h3 class="panel-title">INSTANCE HEALTH STATUS</h3>
      <div class="panel-meta">
        <span class="meta-label">REAL-TIME</span>
        <div class="indicator-dot pulse" />
      </div>
    </div>
    <div v-if="loading" class="panel-loading">
      <div class="loading-text">INITIALIZING...</div>
    </div>
    <div v-else-if="error" class="panel-error">
      <div class="error-text">DATA UNAVAILABLE</div>
    </div>
    <div v-else class="health-grid">
      <div
        v-for="instance in instances"
        :key="instance.name"
        class="health-item"
      >
        <div class="health-header">
          <span class="health-name">{{ instance.name }}</span>
          <div class="health-badge" :class="instance.status">
            {{ instance.status.toUpperCase() }}
          </div>
        </div>
        <div class="health-metrics">
          <div class="metric-row">
            <span class="metric-label">PROVIDER</span>
            <span class="metric-value">{{ instance.provider }}</span>
          </div>
          <div class="metric-row">
            <span class="metric-label">REQUESTS</span>
            <span class="metric-value">{{ formatNumber(instance.requests) }}</span>
          </div>
          <div class="metric-row">
            <span class="metric-label">AVG LATENCY</span>
            <span class="metric-value">{{ formatLatency(instance.latency) }}</span>
          </div>
        </div>
        <div class="health-bar">
          <div
            class="health-fill"
            :class="instance.status"
            :style="{ width: instance.healthPercent + '%' }"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { dashboardApi, type StatsResponse } from '@/api/dashboard'
import { usePolling } from '@/composables/usePolling'

const { data: statsData, isLoading: loading, error } = usePolling<StatsResponse>({
  fn: () => dashboardApi.getStats({ group_by: 'instance' }),
  interval: 5000,
  autoStart: true,
})

interface HealthInstance {
  name: string
  provider: string
  status: 'healthy' | 'unhealthy'
  requests: number
  latency: number
  healthPercent: number
}

const instances = computed<HealthInstance[]>(() => {
  if (!statsData.value?.data) return []

  const healthStatus = statsData.value.data.llm_instance_health_status || {}
  const requests = statsData.value.data.llm_instance_requests_total || {}
  const latencySum = statsData.value.data.llm_request_duration_seconds_sum || {}
  const latencyCount = statsData.value.data.llm_request_duration_seconds_count || {}

  const result: HealthInstance[] = []

  Object.entries(healthStatus).forEach(([key, status]) => {
    // Clean the key by removing escaped quotes
    const cleanKey = key.replace(/"/g, '')

    // Parse instance name from key like 'instance="anthropic-primary"' or just "anthropic-primary"
    const instanceMatch = cleanKey.match(/instance=([^,]+)/)
    const providerMatch = cleanKey.match(/provider=([^,]+)/)

    // If no structured labels, use the whole key as instance name
    const instanceName = instanceMatch ? instanceMatch[1] : cleanKey
    const provider = providerMatch && providerMatch[1] ? providerMatch[1].toUpperCase() : 'UNKNOWN'

    // Find matching request count - search for keys containing the instance name
    const requestKey = Object.keys(requests).find(k => {
      const cleanK = k.replace(/"/g, '')
      return cleanK.includes(instanceName || '')
    })
    const requestCount = requestKey ? requests[requestKey] : 0

    // Calculate average latency
    const latencySumKey = Object.keys(latencySum).find(k => {
      const cleanK = k.replace(/"/g, '')
      return cleanK.includes(instanceName || '')
    })
    const latencyCountKey = Object.keys(latencyCount).find(k => {
      const cleanK = k.replace(/"/g, '')
      return cleanK.includes(instanceName || '')
    })

    const sum = latencySumKey ? latencySum[latencySumKey!] : 0
    const count = latencyCountKey ? latencyCount[latencyCountKey!] : 0
    const avgLatency = count && count > 0 ? sum! / count : 0

    result.push({
      name: instanceName || 'unknown',
      provider,
      status: status === 1 ? 'healthy' : 'unhealthy',
      requests: requestCount || 0,
      latency: avgLatency,
      healthPercent: status === 1 ? 100 : 0,
    })
  })

  return result
})

function formatNumber(num: number): string {
  if (num >= 1000000) {
    return (num / 1000000).toFixed(1) + 'M'
  } else if (num >= 1000) {
    return (num / 1000).toFixed(1) + 'K'
  }
  return num.toString()
}

function formatLatency(seconds: number): string {
  if (seconds < 1) {
    return (seconds * 1000).toFixed(0) + 'ms'
  }
  return seconds.toFixed(2) + 's'
}
</script>

<style scoped>
.chart-panel {
  background: #0a0a0a;
  border: 1px solid #333;
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

.health-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 1px;
  background: #1a1a1a;
}

.health-item {
  background: #0a0a0a;
  padding: 1.5rem;
}

.health-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.health-name {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.85rem;
  font-weight: 700;
  color: #e0e0e0;
}

.health-badge {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 700;
  letter-spacing: 0.1em;
  padding: 0.25rem 0.5rem;
  border-radius: 1px;
}

.health-badge.healthy {
  background: rgba(0, 255, 65, 0.15);
  color: #00ff41;
  border: 1px solid #00ff41;
}

.health-badge.unhealthy {
  background: rgba(255, 0, 65, 0.15);
  color: #ff0041;
  border: 1px solid #ff0041;
}

.health-metrics {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-bottom: 1rem;
}

.metric-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.metric-label {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.6rem;
  font-weight: 500;
  letter-spacing: 0.05em;
  color: #555;
}

.metric-value {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.7rem;
  font-weight: 600;
  color: #999;
}

.health-bar {
  height: 3px;
  background: #1a1a1a;
  overflow: hidden;
}

.health-fill {
  height: 100%;
  transition: width 0.5s ease;
}

.health-fill.healthy {
  background: #00ff41;
  box-shadow: 0 0 8px #00ff41;
}

.health-fill.unhealthy {
  background: #ff0041;
  box-shadow: 0 0 8px #ff0041;
}
</style>
