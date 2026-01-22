<template>
  <div class="routing-rules">
    <div class="panel-header">
      <h3 class="panel-title">ROUTING RULES</h3>
      <div class="panel-controls">
        <button @click="showCreateModal = true" class="btn-primary">
          + Add New Rule
        </button>
        <button @click="loadRules" class="btn-secondary" :disabled="loading">
          ↻ Refresh
        </button>
      </div>
    </div>

    <div v-if="error" class="error-message">{{ error }}</div>

    <div v-if="loading && rules.length === 0" class="loading-message">
      Loading routing rules...
    </div>

    <div v-else-if="rules.length === 0" class="empty-state">
      <p>No routing rules configured.</p>
    </div>

    <div v-else class="table-container">
      <table class="data-table">
        <thead>
          <tr>
            <th>Prefix</th>
            <th>Provider</th>
            <th>Priority</th>
            <th>Status</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="rule in rules" :key="rule.id">
            <td><code>{{ rule.prefix }}</code></td>
            <td>{{ rule.provider }}</td>
            <td>{{ rule.priority }}</td>
            <td>
              <span :class="['status-badge', rule.enabled ? 'status-enabled' : 'status-disabled']">
                {{ rule.enabled ? 'Enabled' : 'Disabled' }}
              </span>
            </td>
            <td>
              <button
                @click="toggleEnabled(rule)"
                class="btn-icon"
                :title="rule.enabled ? 'Disable' : 'Enable'"
              >
                {{ rule.enabled ? '⏸' : '▶' }}
              </button>
              <button
                @click="deleteRule(rule)"
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

    <CreateRoutingRuleModal
      :show="showCreateModal"
      @close="showCreateModal = false"
      @created="handleCreated"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { routingRulesApi, type RoutingRule } from '@/api/config'
import CreateRoutingRuleModal from './CreateRoutingRuleModal.vue'

const rules = ref<RoutingRule[]>([])
const loading = ref(false)
const error = ref('')
const showCreateModal = ref(false)

async function loadRules() {
  loading.value = true
  error.value = ''
  try {
    rules.value = await routingRulesApi.list()
  } catch (err: any) {
    error.value = err.response?.data?.error?.message || 'Failed to load rules'
  } finally {
    loading.value = false
  }
}

async function toggleEnabled(rule: RoutingRule) {
  try {
    await routingRulesApi.update(rule.id, { enabled: !rule.enabled })
    await loadRules()
  } catch (err: any) {
    error.value = err.response?.data?.error?.message || 'Failed to update rule'
  }
}

async function deleteRule(rule: RoutingRule) {
  if (!confirm(`Delete routing rule "${rule.prefix}"?`)) return

  try {
    await routingRulesApi.delete(rule.id)
    await loadRules()
  } catch (err: any) {
    error.value = err.response?.data?.error?.message || 'Failed to delete rule'
  }
}

function handleCreated() {
  showCreateModal.value = false
  loadRules()
}

onMounted(loadRules)
</script>

<style scoped>
.routing-rules {
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

.status-badge {
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
