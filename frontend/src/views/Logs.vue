<template>
  <div class="space-y-6">
    <!-- Header -->
    <div class="flex justify-between items-center">
      <h1 class="text-2xl font-bold text-white">Logs</h1>
      <el-button @click="handleRefresh" :loading="logsStore.loading">
        <el-icon><Refresh /></el-icon>
        Refresh
      </el-button>
    </div>

    <!-- Filters -->
    <div class="grid grid-cols-1 lg:grid-cols-4 gap-6">
      <div class="lg:col-span-1">
        <LogFilters
          :filters="logsStore.filters"
          @apply="handleApplyFilters"
          @reset="handleResetFilters"
        />
      </div>

      <div class="lg:col-span-3">
        <!-- Stats -->
        <div class="glass-strong rounded-xl p-4 mb-4">
          <div class="flex gap-4 text-white">
            <div>
              <span class="text-gray-400">Total: </span>
              <span class="font-bold">{{ logsStore.total }}</span>
            </div>
            <div>
              <span class="text-gray-400">Showing: </span>
              <span class="font-bold">{{ logsStore.logs.length }}</span>
            </div>
            <div v-if="logsStore.error" class="text-red-400">
              Error: {{ logsStore.error.message }}
            </div>
          </div>
        </div>

        <!-- Logs Table -->
        <LogsTable
          :logs="logsStore.logs"
          :loading="logsStore.loading"
          height="600px"
          @row-click="handleRowClick"
        />
      </div>
    </div>

    <!-- Log Detail Dialog -->
    <LogDetail v-model="detailVisible" :log="selectedLog" />

    <!-- Pagination -->
    <div v-if="logsStore.total > (logsStore.filters.limit || 100)" class="glass-strong rounded-xl p-4">
      <el-pagination
        v-model:current-page="currentPage"
        :page-size="logsStore.filters.limit || 100"
        :total="logsStore.total"
        layout="total, prev, pager, next"
        @current-change="handlePageChange"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { useLogsStore } from '@/stores/logs'
import type { LogsQueryParams } from '@/api/logs'
import LogsTable from '@/components/logs/LogsTable.vue'
import LogFilters from '@/components/logs/LogFilters.vue'
import LogDetail from '@/components/logs/LogDetail.vue'
import type { LogEntry } from '@/api/logs'

const logsStore = useLogsStore()

const currentPage = ref(1)
const detailVisible = ref(false)
const selectedLog = ref<LogEntry>({
  timestamp: '',
  level: 'INFO',
  target: '',
  message: '',
  fields: '{}',
})

async function fetchLogs() {
  await logsStore.fetchLogs({
    limit: logsStore.filters.limit,
    level: logsStore.filters.level,
    since_seconds: logsStore.filters.since_seconds,
    grep: logsStore.filters.grep,
  })
}

function handleRefresh() {
  fetchLogs()
}

function handleApplyFilters(filters: LogsQueryParams) {
  currentPage.value = 1
  logsStore.updateFilters(filters)
  fetchLogs()
}

function handleResetFilters() {
  currentPage.value = 1
  logsStore.resetFilters()
  fetchLogs()
}

function handlePageChange(_page: number) {
  // For now, just fetch with limit
  // TODO: Implement offset in backend API
  fetchLogs()
}

function handleRowClick(log: LogEntry) {
  selectedLog.value = log
  detailVisible.value = true
}

onMounted(() => {
  fetchLogs()
})
</script>
