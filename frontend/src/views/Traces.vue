<template>
  <div class="space-y-6">
    <!-- Header -->
    <div class="flex justify-between items-center">
      <h1 class="text-2xl font-bold text-white">Request Traces</h1>
      <el-button @click="handleRefresh" :loading="tracesStore.loading">
        <el-icon><Refresh /></el-icon>
        Refresh
      </el-button>
    </div>

    <!-- Stats -->
    <div class="glass-strong rounded-xl p-4">
      <div class="flex gap-4 text-white">
        <div>
          <span class="text-gray-400">Total: </span>
          <span class="font-bold">{{ tracesStore.total }}</span>
        </div>
        <div v-if="tracesStore.error" class="text-red-400">
          Error: {{ tracesStore.error.message }}
        </div>
      </div>
    </div>

    <!-- Traces Table -->
    <el-card class="glass-strong" shadow="hover">
      <el-table
        :data="tracesStore.traces"
        stripe
        v-loading="tracesStore.loading"
        :header-cell-style="{ background: 'rgba(255,255,255,0.1)', color: '#fff' }"
        :cell-style="{ color: 'rgba(255,255,255,0.8)' }"
        @row-click="handleRowClick"
        class="traces-table"
      >
        <el-table-column prop="request_id" label="Request ID" width="200">
          <template #default="{ row }">
            <el-link type="primary" @click="navigateToDetail(row.request_id)" class="text-white">
              {{ formatRequestId(row.request_id) }}
            </el-link>
          </template>
        </el-table-column>

        <el-table-column prop="model" label="Model" width="200" />

        <el-table-column prop="provider" label="Provider" width="120">
          <template #default="{ row }">
            <span class="capitalize">{{ row.provider }}</span>
          </template>
        </el-table-column>

        <el-table-column prop="duration_ms" label="Duration" width="120" align="right">
          <template #default="{ row }">
            {{ formatDuration(row.duration_ms) }}
          </template>
        </el-table-column>

        <el-table-column prop="status" label="Status" width="100">
          <template #default="{ row }">
            <el-tag :type="row.status === 'success' ? 'success' : 'danger'" size="small">
              {{ row.status }}
            </el-tag>
          </template>
        </el-table-column>

        <el-table-column prop="start_time" label="Time" width="180">
          <template #default="{ row }">
            {{ formatTimestamp(row.start_time) }}
          </template>
        </el-table-column>

        <el-table-column label="Tokens" width="120" align="right">
          <template #default="{ row }">
            <span v-if="row.input_tokens || row.output_tokens">
              {{ row.input_tokens || 0 }}/{{ row.output_tokens || 0 }}
            </span>
            <span v-else>-</span>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { Refresh } from '@element-plus/icons-vue'
import { useTracesStore } from '@/stores/traces'
import dayjs from 'dayjs'

const router = useRouter()
const tracesStore = useTracesStore()

async function fetchTraces() {
  await tracesStore.fetchTraces()
}

function handleRefresh() {
  fetchTraces()
}

function handleRowClick(row: any) {
  navigateToDetail(row.request_id)
}

function navigateToDetail(requestId: string) {
  router.push(`/traces/${requestId}`)
}

function formatRequestId(requestId: string): string {
  if (!requestId) return ''
  return `${requestId.slice(0, 12)}...`
}

function formatDuration(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)}μs`
  if (ms < 1000) return `${ms.toFixed(2)}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function formatTimestamp(timestamp: string): string {
  return dayjs(timestamp).format('YYYY-MM-DD HH:mm:ss')
}

onMounted(() => {
  fetchTraces()
})
</script>

<style scoped>
.traces-table :deep(.el-table__row) {
  cursor: pointer;
}

.traces-table :deep(.el-table__row:hover) {
  background: rgba(255, 255, 255, 0.15) !important;
}
</style>
