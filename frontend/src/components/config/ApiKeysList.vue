<template>
  <div class="api-keys-list">
    <div class="panel-header">
      <h3 class="panel-title">API KEYS</h3>
      <div class="panel-controls">
        <button @click="refreshData" class="btn-secondary" :disabled="loading">
          <span v-if="!loading">↻ Refresh</span>
          <span v-else>Loading...</span>
        </button>
        <button @click="showCreateModal = true" class="btn-primary">
          + Add API Key
        </button>
      </div>
    </div>

    <div v-if="error" class="error-message">
      {{ error }}
    </div>

    <div v-if="loading && apiKeys.length === 0" class="loading-message">
      Loading API keys...
    </div>

    <div v-else-if="apiKeys.length === 0" class="empty-state">
      <p>No API keys configured.</p>
      <p class="hint">Click "Add API Key" to create your first API key.</p>
    </div>

    <div v-else class="table-container">
      <table class="data-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Key Prefix</th>
            <th>Status</th>
            <th>Created</th>
            <th>Last Used</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="key in apiKeys" :key="key.id">
            <td>
              <span class="key-name">{{ key.name }}</span>
              <span v-if="key.description" class="key-description">{{ key.description }}</span>
            </td>
            <td>
              <code class="key-prefix">{{ key.key_prefix }}...</code>
            </td>
            <td>
              <span :class="['status-badge', key.enabled ? 'status-enabled' : 'status-disabled']">
                {{ key.enabled ? 'Enabled' : 'Disabled' }}
              </span>
            </td>
            <td>{{ formatDate(key.created_at) }}</td>
            <td>{{ formatRelativeTime(key.last_used_at) }}</td>
            <td>
              <div class="action-buttons">
                <button
                  @click="toggleEnabled(key)"
                  class="btn-icon"
                  :title="key.enabled ? 'Disable' : 'Enable'"
                >
                  {{ key.enabled ? '⏸' : '▶' }}
                </button>
                <button
                  @click="confirmDelete(key)"
                  class="btn-icon btn-danger"
                  title="Delete"
                >
                  ✕
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Create Modal -->
    <CreateApiKeyModal
      v-if="showCreateModal"
      @close="showCreateModal = false"
      @created="handleCreated"
    />

    <!-- Delete Confirmation Modal -->
    <div v-if="keyToDelete" class="modal-overlay" @click.self="keyToDelete = null">
      <div class="modal-dialog">
        <div class="modal-header">
          <h3>Delete API Key</h3>
        </div>
        <div class="modal-body">
          <p>Are you sure you want to delete the API key "<strong>{{ keyToDelete.name }}</strong>"?</p>
          <p class="warning">This action cannot be undone. All requests using this key will be rejected.</p>
        </div>
        <div class="modal-footer">
          <button @click="keyToDelete = null" class="btn-secondary">Cancel</button>
          <button @click="executeDelete" class="btn-danger" :disabled="deleting">
            {{ deleting ? 'Deleting...' : 'Delete' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { apiKeysApi, formatDate, formatRelativeTime, type ApiKey } from '@/api/config'
import CreateApiKeyModal from './CreateApiKeyModal.vue'

const apiKeys = ref<ApiKey[]>([])
const loading = ref(false)
const error = ref('')
const showCreateModal = ref(false)
const keyToDelete = ref<ApiKey | null>(null)
const deleting = ref(false)

async function fetchData() {
  loading.value = true
  error.value = ''

  try {
    apiKeys.value = await apiKeysApi.list()
  } catch (err: any) {
    console.error('Failed to fetch API keys:', err)
    error.value = err.response?.data?.error?.message || 'Failed to load API keys'
  } finally {
    loading.value = false
  }
}

async function toggleEnabled(key: ApiKey) {
  try {
    await apiKeysApi.update(key.name, { enabled: !key.enabled })
    await fetchData()
  } catch (err: any) {
    console.error('Failed to update API key:', err)
    error.value = err.response?.data?.error?.message || 'Failed to update API key'
  }
}

function confirmDelete(key: ApiKey) {
  keyToDelete.value = key
}

async function executeDelete() {
  if (!keyToDelete.value) return

  deleting.value = true
  try {
    await apiKeysApi.delete(keyToDelete.value.name)
    keyToDelete.value = null
    await fetchData()
  } catch (err: any) {
    console.error('Failed to delete API key:', err)
    error.value = err.response?.data?.error?.message || 'Failed to delete API key'
  } finally {
    deleting.value = false
  }
}

function refreshData() {
  fetchData()
}

function handleCreated() {
  showCreateModal.value = false
  fetchData()
}

onMounted(() => {
  fetchData()
})
</script>

<style scoped>
.api-keys-list {
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

.btn-primary,
.btn-secondary,
.btn-danger {
  padding: 0.5rem 1rem;
  border: none;
  border-radius: 4px;
  font-size: 0.875rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-primary {
  background: #4a9eff;
  color: white;
}

.btn-primary:hover {
  background: #3a8eef;
}

.btn-secondary {
  background: #2a2a2a;
  color: #e5e5e5;
}

.btn-secondary:hover {
  background: #3a3a3a;
}

.btn-secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-danger {
  background: #e74c3c;
  color: white;
}

.btn-danger:hover {
  background: #c0392b;
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

.empty-state .hint {
  margin-top: 0.5rem;
  font-size: 0.875rem;
  color: #666;
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

.key-name {
  font-weight: 600;
  color: #e5e5e5;
}

.key-description {
  display: block;
  margin-top: 0.25rem;
  font-size: 0.75rem;
  color: #666;
}

.key-prefix {
  background: #2a2a2a;
  padding: 0.25rem 0.5rem;
  border-radius: 3px;
  font-family: 'Monaco', 'Courier New', monospace;
  font-size: 0.75rem;
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

.action-buttons {
  display: flex;
  gap: 0.5rem;
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
}

.btn-icon:hover {
  background: #3a3a3a;
}

.btn-icon.btn-danger {
  background: #2c1010;
  color: #e74c3c;
}

.btn-icon.btn-danger:hover {
  background: #4a1d1d;
}

/* Modal Styles */
.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.8);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal-dialog {
  background: #1a1a1a;
  border: 1px solid #2a2a2a;
  border-radius: 8px;
  max-width: 500px;
  width: 90%;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5);
}

.modal-header {
  padding: 1.5rem;
  border-bottom: 1px solid #2a2a2a;
}

.modal-header h3 {
  margin: 0;
  font-size: 1.25rem;
  color: #e5e5e5;
}

.modal-body {
  padding: 1.5rem;
}

.modal-body p {
  margin: 0 0 1rem 0;
  color: #ccc;
}

.modal-body p:last-child {
  margin-bottom: 0;
}

.modal-body .warning {
  color: #e74c3c;
  font-size: 0.875rem;
}

.modal-footer {
  padding: 1.5rem;
  border-top: 1px solid #2a2a2a;
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
}
</style>
