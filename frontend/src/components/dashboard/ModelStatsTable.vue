<template>
  <div class="model-stats-panel">
    <!-- Header -->
    <div class="panel-header">
      <h3 class="panel-title">MODEL STATISTICS</h3>
      <div class="panel-meta">
        <span class="meta-label">TODAY</span>
      </div>
    </div>

    <!-- Table -->
    <div class="table-container">
      <table class="model-stats-table">
        <thead>
          <tr>
            <th class="col-model">MODEL</th>
            <th class="col-requests">REQUESTS</th>
            <th class="col-tokens">TOTAL TOKENS</th>
            <th class="col-tokens">INPUT</th>
            <th class="col-tokens">OUTPUT</th>
            <th class="col-tokens">CACHE CREATE</th>
            <th class="col-tokens">CACHE READ</th>
            <th class="col-percentage">PERCENTAGE</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="models.length === 0">
            <td colspan="8" class="empty-state">
              <span class="empty-text">No data available for today</span>
            </td>
          </tr>
          <tr v-for="model in models" :key="model.model" class="table-row">
            <td class="col-model">
              <span class="model-name">{{ model.model }}</span>
            </td>
            <td class="col-requests">
              <span class="requests-text">{{ formatNumber(model.requests) }}</span>
            </td>
            <td class="col-tokens">
              <span class="tokens-text">{{ formatNumber(model.tokens) }}</span>
            </td>
            <td class="col-tokens">
              <span class="tokens-text-secondary">{{ formatNumber(model.input_tokens) }}</span>
            </td>
            <td class="col-tokens">
              <span class="tokens-text-secondary">{{ formatNumber(model.output_tokens) }}</span>
            </td>
            <td class="col-tokens">
              <span class="tokens-text-cache">{{ formatNumber(model.cache_creation_input_tokens) }}</span>
            </td>
            <td class="col-tokens">
              <span class="tokens-text-cache">{{ formatNumber(model.cache_read_input_tokens) }}</span>
            </td>
            <td class="col-percentage">
              <div class="percentage-container">
                <span class="percentage-text">{{ formatPercentage(model.percentage) }}</span>
                <div class="percentage-bar">
                  <div class="percentage-fill" :style="{ width: `${model.percentage}%` }" />
                </div>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Footer -->
    <div class="panel-footer">
      <div class="footer-stat">
        <span class="footer-label">TOTAL REQUESTS</span>
        <span class="footer-value">{{ formatNumber(totalRequests) }}</span>
      </div>
      <div class="footer-divider">|</div>
      <div class="footer-stat">
        <span class="footer-label">TOTAL TOKENS</span>
        <span class="footer-value">{{ formatNumber(totalTokens) }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { dashboardApi, type ModelStat } from '@/api/dashboard'

const models = ref<ModelStat[]>([])
const totalRequests = ref(0)
const totalTokens = ref(0)
const isLoading = ref(false)

async function fetchData() {
  isLoading.value = true
  try {
    const data = await dashboardApi.getModelsStats()
    models.value = data.models
    totalRequests.value = data.total_requests
    totalTokens.value = data.total_tokens
  } catch (error) {
    console.error('Failed to fetch model stats:', error)
  } finally {
    isLoading.value = false
  }
}

// 初始加载
onMounted(() => {
  fetchData()
})

// 暴露 refresh 方法供父组件调用
defineExpose({
  refresh: fetchData
})

function formatNumber(num: number): string {
  if (num >= 1000000) {
    return `${(num / 1000000).toFixed(1)}M`
  } else if (num >= 1000) {
    return `${(num / 1000).toFixed(1)}K`
  }
  return num.toLocaleString()
}

function formatPercentage(percentage: number): string {
  return `${percentage.toFixed(1)}%`
}
</script>

<style scoped>
/* 使用与 ProviderHealthTable 一致的暗黑主题样式 */
.model-stats-panel {
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

.table-container {
  overflow-x: auto;
}

.model-stats-table {
  width: 100%;
  border-collapse: collapse;
}

.model-stats-table thead {
  background: #0f0f0f;
}

.model-stats-table th {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.15em;
  color: #555;
  text-align: left;
  padding: 0.75rem 1.5rem;
  border-bottom: 1px solid #1a1a1a;
}

.model-stats-table tbody tr {
  border-bottom: 1px solid #1a1a1a;
  transition: background 0.2s;
}

.model-stats-table tbody tr:hover {
  background: #0f0f0f;
}

.model-stats-table td {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.7rem;
  padding: 1rem 1.5rem;
}

.col-model {
  width: 25%;
}

.model-name {
  color: #999;
  font-weight: 500;
}

.col-requests {
  width: 10%;
}

.requests-text {
  color: #999;
}

.col-tokens {
  width: 10%;
}

.tokens-text {
  color: #00d9ff;
  font-weight: 600;
}

.tokens-text-secondary {
  color: #999;
}

.tokens-text-cache {
  color: #9966ff;
  font-weight: 500;
}

.col-percentage {
  width: 15%;
}

.percentage-container {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.percentage-text {
  color: #00ff41;
  font-size: 0.65rem;
  font-weight: 600;
}

.percentage-bar {
  width: 100%;
  height: 4px;
  background: #1a1a1a;
  border-radius: 2px;
  overflow: hidden;
}

.percentage-fill {
  height: 100%;
  background: linear-gradient(90deg, #00ff41 0%, #00d9ff 100%);
  transition: width 0.3s ease;
}

.empty-state {
  text-align: center;
  padding: 2rem 1.5rem !important;
}

.empty-text {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.65rem;
  color: #555;
  font-style: italic;
}

.panel-footer {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem 1.5rem;
  border-top: 1px solid #1a1a1a;
  background: #0f0f0f;
}

.footer-stat {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.footer-label {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: #444;
}

.footer-value {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.65rem;
  font-weight: 600;
  color: #00ff41;
}

.footer-divider {
  font-size: 0.7rem;
  color: #222;
  margin: 0 0.25rem;
}
</style>
