<template>
  <div class="dashboard-container">
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

const uptime = ref('00:00:00')
const lastUpdate = ref('-')
let startTime = Date.now()
let uptimeInterval: number

function updateUptime() {
  const elapsed = Date.now() - startTime
  const seconds = Math.floor(elapsed / 1000)
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const secs = seconds % 60

  uptime.value = `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`

  // Also update lastUpdate time
  const now = new Date()
  lastUpdate.value = now.toLocaleTimeString()
}

onMounted(() => {
  updateUptime()
  uptimeInterval = window.setInterval(updateUptime, 1000)
})

onUnmounted(() => {
  clearInterval(uptimeInterval)
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
}
</style>
