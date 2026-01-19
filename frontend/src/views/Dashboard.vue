<template>
  <div class="dashboard-container">
    <!-- Header Section -->
    <header class="dashboard-header">
      <div class="header-left">
        <h1 class="header-title">LLM GATEWAY</h1>
        <div class="header-divider">//</div>
        <div class="header-subtitle">MONITORING CONSOLE</div>
      </div>
      <div class="header-right">
        <div class="timestamp">{{ currentTime }}</div>
        <div class="status-badge" :class="{ online: isOnline }">
          {{ isOnline ? 'CONNECTED' : 'DISCONNECTED' }}
        </div>
      </div>
    </header>

    <!-- Summary Cards -->
    <SummaryCards />

    <!-- Main Content Grid -->
    <div class="main-grid">
      <!-- Left Column -->
      <div class="column-left">
        <TokenUsageByApiKey />
        <TokenUsageByInstance />
      </div>

      <!-- Right Column -->
      <div class="column-right">
        <ProviderHealthTable />
      </div>
    </div>

    <!-- Footer -->
    <footer class="dashboard-footer">
      <div class="footer-section">
        <span class="footer-label">VERSION</span>
        <span class="footer-value">0.3.0</span>
      </div>
      <div class="footer-divider">|</div>
      <div class="footer-section">
        <span class="footer-label">UPTIME</span>
        <span class="footer-value">{{ uptime }}</span>
      </div>
      <div class="footer-divider">|</div>
      <div class="footer-section">
        <span class="footer-label">LAST UPDATE</span>
        <span class="footer-value">{{ lastUpdate }}</span>
      </div>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import SummaryCards from '@/components/dashboard/SummaryCards.vue'
import TokenUsageByApiKey from '@/components/dashboard/TokenUsageByApiKey.vue'
import TokenUsageByInstance from '@/components/dashboard/TokenUsageByInstance.vue'
import ProviderHealthTable from '@/components/dashboard/ProviderHealthTable.vue'

const currentTime = ref('')
const isOnline = ref(navigator.onLine)
const uptime = ref('00:00:00')
const lastUpdate = ref('-')
let startTime = Date.now()
let timeInterval: number
let uptimeInterval: number

function updateTime() {
  const now = new Date()
  currentTime.value = now.toUTCString().replace('GMT', 'UTC')
  lastUpdate.value = now.toTimeString().split(' ')[0]
}

function updateUptime() {
  const elapsed = Date.now() - startTime
  const seconds = Math.floor(elapsed / 1000)
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const secs = seconds % 60

  uptime.value = `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`
}

function handleOnlineStatus() {
  isOnline.value = navigator.onLine
}

onMounted(() => {
  updateTime()
  updateUptime()

  timeInterval = window.setInterval(updateTime, 1000)
  uptimeInterval = window.setInterval(updateUptime, 1000)

  window.addEventListener('online', handleOnlineStatus)
  window.addEventListener('offline', handleOnlineStatus)
})

onUnmounted(() => {
  clearInterval(timeInterval)
  clearInterval(uptimeInterval)

  window.removeEventListener('online', handleOnlineStatus)
  window.removeEventListener('offline', handleOnlineStatus)
})
</script>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&display=swap');

.dashboard-container {
  min-height: 100vh;
  background: #0a0a0a;
  padding: 1.5rem;
  font-family: 'IBM Plex Sans', monospace;
}

/* Header */
.dashboard-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 2rem;
  padding-bottom: 1.5rem;
  border-bottom: 1px solid #1a1a1a;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.header-title {
  font-family: 'JetBrains Mono', monospace;
  font-size: 1.25rem;
  font-weight: 700;
  color: #e0e0e0;
  letter-spacing: 0.05em;
  margin: 0;
}

.header-divider {
  font-size: 1rem;
  color: #333;
  font-weight: 300;
}

.header-subtitle {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.65rem;
  font-weight: 600;
  letter-spacing: 0.2em;
  color: #444;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.timestamp {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.7rem;
  font-weight: 500;
  color: #555;
  letter-spacing: 0.05em;
}

.status-badge {
  font-family: 'IBM Plex Sans', monospace;
  font-size: 0.55rem;
  font-weight: 700;
  letter-spacing: 0.1em;
  padding: 0.35rem 0.75rem;
  border: 1px solid #333;
  color: #555;
}

.status-badge.online {
  border-color: #00ff41;
  color: #00ff41;
  background: rgba(0, 255, 65, 0.08);
}

/* Main Grid */
.main-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
  margin-bottom: 1.5rem;
}

.column-left,
.column-right {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

/* Footer */
.dashboard-footer {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding-top: 1rem;
  border-top: 1px solid #1a1a1a;
}

.footer-section {
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
  font-weight: 500;
  color: #666;
}

.footer-divider {
  font-size: 0.7rem;
  color: #222;
  margin: 0 0.25rem;
}

/* CRT Scanline Effect */
.dashboard-container::before {
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

/* Responsive */
@media (max-width: 1024px) {
  .main-grid {
    grid-template-columns: 1fr;
  }

  .dashboard-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 1rem;
  }
}
</style>
