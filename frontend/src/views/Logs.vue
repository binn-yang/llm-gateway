<template>
  <div class="logs-container">
    <!-- Stats Bar -->
    <div class="stats-bar">
      <div class="stat-item">
        <span class="stat-label">LOGS</span>
        <span class="stat-value">{{ logsStore.total }}</span>
        <span class="stat-unit">TOTAL</span>
      </div>
      <div class="stat-divider" />
      <div class="stat-item">
        <span class="stat-label">SHOWING</span>
        <span class="stat-value">{{ logsStore.logs.length }}</span>
      </div>
      <div class="stat-divider" />
      <div class="stat-item" v-if="logsStore.filesSearched.length > 0">
        <span class="stat-label">FILES</span>
        <span class="stat-value">{{ logsStore.filesSearched.join(', ') }}</span>
      </div>
      <div class="stat-spacer" />
      <button class="refresh-btn" @click="handleRefresh" :disabled="logsStore.loading">
        <span class="btn-text">{{ logsStore.loading ? 'LOADING...' : 'REFRESH' }}</span>
      </button>
    </div>

    <!-- Error Alert -->
    <div v-if="logsStore.error" class="error-alert">
      <span class="error-label">ERROR:</span>
      <span class="error-text">{{ logsStore.error }}</span>
      <button class="error-close" @click="logsStore.error = null">×</button>
    </div>

    <!-- Main Grid -->
    <div class="main-grid">
      <!-- Filters Panel -->
      <div class="filters-panel">
        <LogFilters
          :filters="logsStore.filters"
          :files-searched="logsStore.filesSearched"
          @apply="handleApplyFilters"
          @reset="handleResetFilters"
        />
      </div>

      <!-- Logs Table Panel -->
      <div class="table-panel">
        <LogsTable
          :logs="logsStore.logs"
          :loading="logsStore.loading"
          @row-click="handleRowClick"
          @view-trace="handleViewTrace"
        />
      </div>
    </div>

    <!-- Log Detail Dialog -->
    <LogDetail v-model="detailVisible" :log="selectedLog" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useLogsStore } from '@/stores/logs'
import type { LogsQueryParams, LogEntry } from '@/api/logs'
import LogsTable from '@/components/logs/LogsTable.vue'
import LogFilters from '@/components/logs/LogFilters.vue'
import LogDetail from '@/components/logs/LogDetail.vue'

const logsStore = useLogsStore()

const detailVisible = ref(false)
const selectedLog = ref<LogEntry | null>(null)

function handleRefresh() {
  logsStore.fetchLogs()
}

function handleApplyFilters(filters: LogsQueryParams) {
  logsStore.updateFilters(filters)
  logsStore.fetchLogs()
}

function handleResetFilters() {
  logsStore.resetFilters()
  logsStore.fetchLogs()
}

function handleRowClick(log: LogEntry) {
  selectedLog.value = log
  detailVisible.value = true
}

function handleViewTrace(requestId: string) {
  // 临时方案：在日志中搜索该 request_id
  logsStore.updateFilters({ request_id: requestId, limit: 100 })
  logsStore.fetchLogs()
}

onMounted(() => {
  logsStore.fetchLogs()
})
</script>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&display=swap');

.logs-container {
  min-height: 100vh;
  background: #0a0a0a;
  padding: 1.5rem;
  font-family: 'IBM Plex Sans', monospace;
}

/* CRT Scanline Effect */
.logs-container::before {
  content: '';
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: repeating-linear-gradient(
    0deg,
    rgba(0, 0, 0, 0.15),
    rgba(0, 0, 0, 0.15) 1px,
    transparent 1px,
    transparent 2px
  );
  pointer-events: none;
  z-index: 1000;
  opacity: 0.3;
}

/* Stats Bar */
.stats-bar {
  display: flex;
  align-items: center;
  padding: 1rem 1.5rem;
  background: #0a0a0a;
  border: 1px solid #333;
  margin-bottom: 1rem;
}

.stat-item {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
}

.stat-label {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.15em;
  color: #666;
}

.stat-value {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.75rem;
  font-weight: 600;
  color: #e0e0e0;
}

.stat-unit {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.5rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: #555;
}

.stat-divider {
  width: 1px;
  height: 16px;
  background: #333;
  margin: 0 1rem;
}

.stat-spacer {
  flex: 1;
}

.refresh-btn {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.15em;
  padding: 0.5rem 1rem;
  background: #0a0a0a;
  border: 1px solid #333;
  color: #666;
  cursor: pointer;
  transition: all 0.2s;
}

.refresh-btn:hover:not(:disabled) {
  border-color: #00ff41;
  color: #00ff41;
  box-shadow: 0 0 8px rgba(0, 255, 65, 0.3);
}

.refresh-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Error Alert */
.error-alert {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: rgba(255, 0, 65, 0.08);
  border: 1px solid #ff0041;
  margin-bottom: 1rem;
}

.error-label {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 700;
  letter-spacing: 0.15em;
  color: #ff0041;
}

.error-text {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.65rem;
  color: #ff0041;
  flex: 1;
}

.error-close {
  font-family: 'JetBrains Mono', monospace;
  font-size: 1.2rem;
  background: none;
  border: none;
  color: #ff0041;
  cursor: pointer;
  padding: 0;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* Main Grid */
.main-grid {
  display: grid;
  grid-template-columns: 300px 1fr;
  gap: 1rem;
}

.filters-panel {
  background: #0a0a0a;
  border: 1px solid #333;
}

.table-panel {
  background: #0a0a0a;
  border: 1px solid #333;
}

/* Responsive */
@media (max-width: 1024px) {
  .main-grid {
    grid-template-columns: 1fr;
  }

  .stats-bar {
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .stat-divider {
    display: none;
  }
}
</style>
