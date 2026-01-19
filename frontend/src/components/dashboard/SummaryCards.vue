<template>
  <div class="summary-grid">
    <div
      v-for="card in cards"
      :key="card.key"
      class="metric-card"
      :class="{ [`status-${card.status}`]: true }"
    >
      <div class="card-header">
        <span class="card-label">{{ card.label }}</span>
        <div v-if="card.indicator" class="status-indicator" :class="card.indicator" />
      </div>
      <div class="card-value">
        <span class="value-text">{{ displayValue(card) }}</span>
        <span v-if="card.unit" class="value-unit">{{ card.unit }}</span>
      </div>
      <div v-if="card.description" class="card-description">
        {{ card.description }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { usePolling } from '@/composables/usePolling'
import { dashboardApi, type DashboardSummary } from '@/api/dashboard'

const { data: summaryData } = usePolling<DashboardSummary>({
  fn: () => dashboardApi.getSummary(),
  interval: 5000,
  autoStart: true,
})

interface MetricCard {
  key: string
  label: string
  value: number | boolean
  unit?: string
  description?: string
  status: 'ok' | 'warning' | 'error' | 'neutral'
  indicator?: 'pulse' | 'blink' | 'static'
}

const cards = computed<MetricCard[]>(() => {
  const data = summaryData.value
  if (!data) return []

  const healthStatus = data.health_status ? 'ok' : 'error'

  return [
    {
      key: 'api_keys',
      label: 'API_KEYS',
      value: data.api_key_count,
      unit: 'TOTAL',
      status: 'neutral',
    },
    {
      key: 'providers',
      label: 'PROVIDERS',
      value: data.provider_count,
      unit: 'ACTIVE',
      status: data.provider_count > 0 ? 'ok' : 'warning',
    },
    {
      key: 'today_requests',
      label: 'REQUESTS',
      value: data.today_requests,
      unit: 'TODAY',
      status: 'neutral',
      description: formatNumber(data.total_requests) + ' TOTAL',
    },
    {
      key: 'today_tokens',
      label: 'TOKENS',
      value: data.today_tokens,
      unit: 'TODAY',
      status: 'neutral',
      description: formatNumber(data.total_tokens) + ' CUMULATIVE',
    },
    {
      key: 'total_tokens',
      label: 'TOKENS',
      value: data.total_tokens,
      unit: 'ALL TIME',
      status: 'neutral',
    },
    {
      key: 'health',
      label: 'SYSTEM',
      value: data.health_status,
      status: healthStatus,
      indicator: data.health_status ? 'pulse' : 'blink',
      description: data.health_status ? 'OPERATIONAL' : 'DEGRADED',
    },
  ]
})

function displayValue(card: MetricCard): string {
  if (typeof card.value === 'boolean') {
    return card.value ? 'ONLINE' : 'OFFLINE'
  }
  return formatNumber(card.value)
}

function formatNumber(num: number): string {
  if (num >= 1000000) {
    return (num / 1000000).toFixed(1) + 'M'
  } else if (num >= 1000) {
    return (num / 1000).toFixed(1) + 'K'
  }
  return num.toString()
}
</script>

<style scoped>
.summary-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 1px;
  background: #1a1a1a;
  border: 1px solid #333;
  margin-bottom: 2rem;
}

.metric-card {
  background: #0a0a0a;
  padding: 1.5rem;
  position: relative;
  overflow: hidden;
}

.metric-card::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 2px;
  background: #333;
}

.metric-card.status-ok::before {
  background: #00ff41;
}

.metric-card.status-warning::before {
  background: #ff6b00;
}

.metric-card.status-error::before {
  background: #ff0041;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.card-label {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.65rem;
  font-weight: 600;
  letter-spacing: 0.15em;
  color: #666;
}

.status-indicator {
  width: 8px;
  height: 8px;
  border-radius: 1px;
  background: #333;
}

.status-indicator.pulse {
  background: #00ff41;
  animation: pulse 2s ease-in-out infinite;
}

.status-indicator.blink {
  background: #ff0041;
  animation: blink 1s steps(1) infinite;
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

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.card-value {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.value-text {
  font-family: 'JetBrains Mono', monospace;
  font-size: 2rem;
  font-weight: 700;
  color: #e0e0e0;
  line-height: 1;
}

.value-unit {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.6rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: #555;
}

.card-description {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.65rem;
  color: #444;
  letter-spacing: 0.05em;
}
</style>
