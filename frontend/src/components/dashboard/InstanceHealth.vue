<template>
  <el-card class="health-card glass-strong" shadow="hover">
    <template #header>
      <span class="text-white font-bold">Provider Instance Health</span>
    </template>

    <div v-if="loading" class="flex justify-center items-center h-48">
      <el-icon class="is-loading text-white text-4xl"><Loading /></el-icon>
    </div>

    <div v-else-if="error" class="flex justify-center items-center h-48">
      <el-alert type="error" :closable="false">
        Failed to load instance health data
      </el-alert>
    </div>

    <el-table
      v-else
      :data="instances"
      stripe
      :header-cell-style="{ background: 'rgba(255,255,255,0.1)', color: '#fff' }"
      :cell-style="{ color: 'rgba(255,255,255,0.8)' }"
    >
      <el-table-column prop="provider" label="Provider" width="120">
        <template #default="{ row }">
          <span class="capitalize">{{ row.provider }}</span>
        </template>
      </el-table-column>

      <el-table-column prop="instance" label="Instance" />

      <el-table-column prop="status" label="Health" width="100" align="center">
        <template #default="{ row }">
          <el-tag :type="row.healthy ? 'success' : 'danger'" size="small">
            {{ row.healthy ? 'Healthy' : 'Unhealthy' }}
          </el-tag>
        </template>
      </el-table-column>

      <el-table-column prop="successRate" label="Success Rate" width="150">
        <template #default="{ row }">
          <el-progress
            :percentage="Math.round(row.successRate * 100)"
            :color="getRateColor(row.successRate)"
            :stroke-width="8"
          />
        </template>
      </el-table-column>

      <el-table-column prop="requests" label="Requests" width="100" align="right">
        <template #default="{ row }">
          {{ formatNumber(row.requests) }}
        </template>
      </el-table-column>

      <el-table-column prop="avgLatency" label="Avg Latency" width="100" align="right">
        <template #default="{ row }">
          {{ formatLatency(row.avgLatency) }}
        </template>
      </el-table-column>
    </el-table>
  </el-card>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { Loading } from '@element-plus/icons-vue'

interface InstanceHealth {
  provider: string
  instance: string
  healthy: boolean
  successRate: number
  requests: number
  avgLatency: number
}

const loading = ref(false)
const error = ref(false)
const instances = ref<InstanceHealth[]>([])

let pollInterval: number | null = null

async function fetchHealth() {
  loading.value = true
  error.value = false

  try {
    // For now, use mock data
    // TODO: Replace with actual API call
    instances.value = [
      {
        provider: 'openai',
        instance: 'openai-primary',
        healthy: true,
        successRate: 0.98,
        requests: 1234,
        avgLatency: 0.85,
      },
      {
        provider: 'anthropic',
        instance: 'anthropic-primary',
        healthy: true,
        successRate: 0.99,
        requests: 5678,
        avgLatency: 1.2,
      },
      {
        provider: 'anthropic',
        instance: 'anthropic-backup',
        healthy: true,
        successRate: 0.0,
        requests: 0,
        avgLatency: 0,
      },
      {
        provider: 'gemini',
        instance: 'gemini-primary',
        healthy: false,
        successRate: 0.75,
        requests: 234,
        avgLatency: 2.5,
      },
    ]
  } catch (err) {
    console.error('Failed to fetch instance health:', err)
    error.value = true
  } finally {
    loading.value = false
  }
}

function getRateColor(rate: number): string {
  if (rate >= 0.95) return '#10b981' // success
  if (rate >= 0.8) return '#f59e0b' // warning
  return '#ef4444' // danger
}

function formatNumber(num: number): string {
  if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`
  if (num >= 1000) return `${(num / 1000).toFixed(1)}K`
  return num.toString()
}

function formatLatency(seconds: number): string {
  if (seconds < 1) return `${(seconds * 1000).toFixed(0)}ms`
  return `${seconds.toFixed(2)}s`
}

onMounted(() => {
  fetchHealth()
  pollInterval = window.setInterval(fetchHealth, 5000)
})

onUnmounted(() => {
  if (pollInterval) {
    clearInterval(pollInterval)
    pollInterval = null
  }
})
</script>

<style scoped>
.el-table {
  background: transparent;
}

.el-table :deep(.el-table__row) {
  background: rgba(255, 255, 255, 0.05);
}

.el-table :deep(.el-table__row:hover) {
  background: rgba(255, 255, 255, 0.1) !important;
}

.el-table :deep(.el-table__body tr.current-row) {
  background: rgba(255, 255, 255, 0.15) !important;
}
</style>
