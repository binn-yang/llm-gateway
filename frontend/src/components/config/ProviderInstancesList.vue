<template>
  <div class="provider-instances">
    <div class="panel-header">
      <h3 class="panel-title">PROVIDER INSTANCES</h3>
      <div class="panel-controls">
        <select v-model="selectedProvider" @change="loadInstances" class="provider-select">
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
          <option value="gemini">Gemini</option>
        </select>
        <button @click="showCreateModal = true" class="btn-primary">
          + Add Instance
        </button>
        <button @click="loadInstances" class="btn-secondary" :disabled="loading">
          ↻ Refresh
        </button>
      </div>
    </div>

    <div v-if="error" class="error-message">{{ error }}</div>

    <div v-if="loading && instances.length === 0" class="loading-message">
      Loading provider instances...
    </div>

    <div v-else-if="instances.length === 0" class="empty-state">
      <p>No {{ selectedProvider }} instances configured.</p>
    </div>

    <div v-else class="table-container">
      <table class="data-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Base URL</th>
            <th>Priority</th>
            <th>Status</th>
            <th>Health</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="instance in instances" :key="instance.id">
            <td>
              <span class="instance-name">{{ instance.name }}</span>
              <span v-if="instance.description" class="instance-description">
                {{ instance.description }}
              </span>
            </td>
            <td><code class="base-url">{{ instance.base_url }}</code></td>
            <td>{{ instance.priority }}</td>
            <td>
              <span :class="['status-badge', instance.enabled ? 'status-enabled' : 'status-disabled']">
                {{ instance.enabled ? 'Enabled' : 'Disabled' }}
              </span>
            </td>
            <td>
              <span :class="['health-badge', `health-${instance.health_status || 'unknown'}`]">
                {{ instance.health_status || 'unknown' }}
              </span>
            </td>
            <td>
              <button
                @click="toggleEnabled(instance)"
                class="btn-icon"
                :title="instance.enabled ? 'Disable' : 'Enable'"
              >
                {{ instance.enabled ? '⏸' : '▶' }}
              </button>
              <button
                @click="deleteInstance(instance)"
                class="btn-icon btn-danger"
                title="Delete"
              >
                🗑
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <CreateProviderInstanceModal
      :show="showCreateModal"
      :provider="selectedProvider"
      @close="showCreateModal = false"
      @created="handleCreated"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { providerInstancesApi, type ProviderInstance } from '@/api/config'
import CreateProviderInstanceModal from './CreateProviderInstanceModal.vue'

const selectedProvider = ref('openai')
const instances = ref<ProviderInstance[]>([])
const loading = ref(false)
const error = ref('')
const showCreateModal = ref(false)

async function loadInstances() {
  loading.value = true
  error.value = ''
  try {
    instances.value = await providerInstancesApi.list(selectedProvider.value)
  } catch (err: any) {
    error.value = err.response?.data?.error?.message || 'Failed to load instances'
  } finally {
    loading.value = false
  }
}

async function toggleEnabled(instance: ProviderInstance) {
  try {
    await providerInstancesApi.update(selectedProvider.value, instance.name, {
      enabled: !instance.enabled
    })
    await loadInstances()
  } catch (err: any) {
    error.value = err.response?.data?.error?.message || 'Failed to update instance'
  }
}

async function deleteInstance(instance: ProviderInstance) {
  if (!confirm(`Delete provider instance "${instance.name}"?`)) return

  try {
    await providerInstancesApi.delete(selectedProvider.value, instance.name)
    await loadInstances()
  } catch (err: any) {
    error.value = err.response?.data?.error?.message || 'Failed to delete instance'
  }
}

function handleCreated() {
  showCreateModal.value = false
  loadInstances()
}

onMounted(loadInstances)
</script>

<style scoped>
.provider-instances {
  width: 100%;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
}

.panel-title {
  font-size: 1.25rem;
  font-weight: 700;
  color: #e5e5e5;
  letter-spacing: 0.05em;
  margin: 0;
}

.panel-controls {
  display: flex;
  gap: 0.75rem;
}

.provider-select {
  padding: 0.5rem 1rem;
  background: #2a2a2a;
  border: 1px solid #3a3a3a;
  border-radius: 4px;
  color: #e5e5e5;
  font-size: 0.875rem;
  cursor: pointer;
}

.btn-primary {
  padding: 0.5rem 1rem;
  background: #4a9eff;
  border: none;
  border-radius: 4px;
  color: white;
  font-size: 0.875rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-primary:hover {
  background: #3a8eef;
}

.btn-secondary {
  padding: 0.5rem 1rem;
  background: #2a2a2a;
  border: none;
  border-radius: 4px;
  color: #e5e5e5;
  font-size: 0.875rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-secondary:hover {
  background: #3a3a3a;
}

.error-message {
  padding: 1rem;
  background: #2c1010;
  border: 1px solid #e74c3c;
  border-radius: 4px;
  color: #e74c3c;
  margin-bottom: 1rem;
}

.loading-message,
.empty-state {
  padding: 3rem;
  text-align: center;
  color: #888;
}

.table-container {
  overflow-x: auto;
  border: 1px solid #2a2a2a;
  border-radius: 4px;
}

.data-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.875rem;
}

.data-table thead {
  background: #151515;
}

.data-table th {
  padding: 0.75rem 1rem;
  text-align: left;
  font-weight: 600;
  color: #888;
  text-transform: uppercase;
  font-size: 0.75rem;
  letter-spacing: 0.05em;
  border-bottom: 1px solid #2a2a2a;
}

.data-table td {
  padding: 1rem;
  border-bottom: 1px solid #222;
}

.data-table tbody tr:hover {
  background: #1a1a1a;
}

.instance-name {
  font-weight: 600;
  color: #e5e5e5;
}

.instance-description {
  display: block;
  margin-top: 0.25rem;
  font-size: 0.75rem;
  color: #666;
}

.base-url {
  background: #2a2a2a;
  padding: 0.25rem 0.5rem;
  border-radius: 3px;
  font-family: 'Monaco', 'Courier New', monospace;
  font-size: 0.75rem;
}

.status-badge,
.health-badge {
  display: inline-block;
  padding: 0.25rem 0.75rem;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 600;
}

.status-enabled {
  background: #1a4d2e;
  color: #4ade80;
}

.status-disabled {
  background: #4a1d1d;
  color: #e74c3c;
}

.health-healthy {
  background: #1a4d2e;
  color: #4ade80;
}

.health-unhealthy {
  background: #4a1d1d;
  color: #e74c3c;
}

.health-unknown {
  background: #2a2a2a;
  color: #888;
}

.btn-icon {
  padding: 0.375rem 0.75rem;
  background: #2a2a2a;
  border: none;
  border-radius: 4px;
  color: #e5e5e5;
  font-size: 1rem;
  cursor: pointer;
  transition: all 0.2s;
  margin-right: 0.5rem;
}

.btn-icon:hover {
  background: #3a3a3a;
}

.btn-icon.btn-danger:hover {
  background: #4a1d1d;
  color: #e74c3c;
}
</style>
