<template>
  <div class="health-table-panel">
    <div class="panel-header">
      <h3 class="panel-title">PROVIDER INSTANCE HEALTH</h3>
      <div class="panel-meta">
        <span class="meta-label">REAL-TIME</span>
        <div class="indicator-dot pulse" />
      </div>
    </div>

    <div class="table-container">
      <table class="health-table">
        <thead>
          <tr>
            <th class="col-instance">INSTANCE</th>
            <th class="col-status">STATUS</th>
            <th class="col-duration">DURATION</th>
            <th class="col-downtime">DOWNTIME (24H)</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="instance in instances" :key="`${instance.provider}-${instance.instance}`" class="table-row">
            <td class="col-instance">
              <div class="instance-name">
                <span class="provider-badge">{{ instance.provider.toUpperCase() }}</span>
                <span class="instance-name-text">{{ instance.instance }}</span>
              </div>
            </td>
            <td class="col-status">
              <span class="status-badge" :class="{ healthy: instance.is_healthy, unhealthy: !instance.is_healthy }">
                {{ instance.is_healthy ? 'HEALTHY' : 'UNHEALTHY' }}
              </span>
            </td>
            <td class="col-duration">
              <span class="duration-text">{{ formatDuration(instance.duration_secs) }}</span>
            </td>
            <td class="col-downtime">
              <span class="downtime-text" :class="{ 'has-downtime': instance.downtime_last_24h_secs > 0 }">
                {{ formatDuration(instance.downtime_last_24h_secs) }}
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { dashboardApi, type InstanceHealthDetail } from '@/api/dashboard'
import { usePolling } from '@/composables/usePolling'

const { data: healthData } = usePolling<{ timestamp: string; instances: InstanceHealthDetail[] }>({
  fn: () => dashboardApi.getInstancesHealth(),
  interval: 5000,
  autoStart: true,
})

const instances = ref<InstanceHealthDetail[]>([])

// Watch for data changes
watch(healthData, (newData) => {
  if (newData) {
    instances.value = newData.instances
  }
}, { immediate: true, deep: true })

function formatDuration(secs: number): string {
  if (secs === 0) return '-'

  const hours = Math.floor(secs / 3600)
  const minutes = Math.floor((secs % 3600) / 60)
  const seconds = secs % 60

  if (hours > 0) {
    return `${hours}h ${minutes}m`
  } else if (minutes > 0) {
    return `${minutes}m ${seconds}s`
  } else {
    return `${seconds}s`
  }
}
</script>

<style scoped>
.health-table-panel {
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

.table-container {
  overflow-x: auto;
}

.health-table {
  width: 100%;
  border-collapse: collapse;
}

.health-table thead {
  background: #0f0f0f;
}

.health-table th {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.15em;
  color: #555;
  text-align: left;
  padding: 0.75rem 1.5rem;
  border-bottom: 1px solid #1a1a1a;
}

.health-table tbody tr {
  border-bottom: 1px solid #1a1a1a;
  transition: background 0.2s;
}

.health-table tbody tr:hover {
  background: #0f0f0f;
}

.health-table td {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.7rem;
  padding: 1rem 1.5rem;
}

.col-instance {
  width: 40%;
}

.instance-name {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.provider-badge {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.5rem;
  font-weight: 700;
  letter-spacing: 0.1em;
  padding: 0.15rem 0.4rem;
  background: #1a1a1a;
  color: #666;
  border: 1px solid #333;
}

.instance-name-text {
  color: #999;
}

.col-status {
  width: 20%;
}

.status-badge {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  padding: 0.25rem 0.5rem;
  border: 1px solid;
  display: inline-block;
}

.status-badge.healthy {
  border-color: #00ff41;
  color: #00ff41;
  background: rgba(0, 255, 65, 0.08);
}

.status-badge.unhealthy {
  border-color: #ff0041;
  color: #ff0041;
  background: rgba(255, 0, 65, 0.08);
  animation: blink 1s steps(1) infinite;
}

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.col-duration {
  width: 20%;
}

.duration-text {
  color: #999;
}

.col-downtime {
  width: 20%;
}

.downtime-text {
  color: #999;
}

.downtime-text.has-downtime {
  color: #ff6b00;
  font-weight: 600;
}
</style>
