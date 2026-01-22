<template>
  <div class="config-management">
    <div class="page-header">
      <h1 class="page-title">CONFIGURATION</h1>
      <p class="page-subtitle">Manage API keys, routing rules, and provider instances</p>
    </div>

    <div class="tabs-container">
      <div class="tabs">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          :class="['tab', { active: activeTab === tab.id }]"
          @click="activeTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </div>

      <div class="tab-content">
        <ApiKeysList v-if="activeTab === 'api-keys'" />
        <RoutingRulesList v-if="activeTab === 'routing'" />
        <ProviderInstancesList v-if="activeTab === 'providers'" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import ApiKeysList from '@/components/config/ApiKeysList.vue'
import RoutingRulesList from '@/components/config/RoutingRulesList.vue'
import ProviderInstancesList from '@/components/config/ProviderInstancesList.vue'

const tabs = [
  { id: 'api-keys', label: 'API Keys' },
  { id: 'routing', label: 'Routing' },
  { id: 'providers', label: 'Providers' },
]

const activeTab = ref('api-keys')
</script>

<style scoped>
.config-management {
  padding: 2rem;
  max-width: 1400px;
  margin: 0 auto;
}

.page-header {
  margin-bottom: 2rem;
}

.page-title {
  font-size: 2rem;
  font-weight: 700;
  color: #e5e5e5;
  margin: 0 0 0.5rem 0;
  letter-spacing: 0.05em;
}

.page-subtitle {
  font-size: 1rem;
  color: #888;
  margin: 0;
}

.tabs-container {
  background: #1a1a1a;
  border: 1px solid #2a2a2a;
  border-radius: 8px;
  overflow: hidden;
}

.tabs {
  display: flex;
  border-bottom: 1px solid #2a2a2a;
  background: #151515;
}

.tab {
  padding: 1rem 2rem;
  background: none;
  border: none;
  color: #888;
  font-size: 0.875rem;
  font-weight: 600;
  letter-spacing: 0.05em;
  cursor: pointer;
  transition: all 0.2s;
  border-bottom: 2px solid transparent;
}

.tab:hover {
  color: #e5e5e5;
  background: #1a1a1a;
}

.tab.active {
  color: #4a9eff;
  border-bottom-color: #4a9eff;
  background: #1a1a1a;
}

.tab-content {
  padding: 2rem;
  min-height: 500px;
}
</style>
