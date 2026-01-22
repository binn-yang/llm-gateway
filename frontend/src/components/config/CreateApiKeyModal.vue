<template>
  <div class="modal-overlay" @click.self="$emit('close')">
    <div class="modal-dialog">
      <div class="modal-header">
        <h3>Create API Key</h3>
        <button @click="$emit('close')" class="close-btn">✕</button>
      </div>

      <form @submit.prevent="handleSubmit">
        <div class="modal-body">
          <div v-if="error" class="error-message">
            {{ error }}
          </div>

          <!-- Success state - show generated key -->
          <div v-if="createdKey" class="success-state">
            <div class="success-icon">✓</div>
            <h4>API Key Created Successfully!</h4>

            <div class="key-display">
              <label>Your API Key (save it now!):</label>
              <div class="key-box">
                <code>{{ formData.key }}</code>
                <button
                  type="button"
                  @click="copyToClipboard"
                  class="copy-btn"
                  :title="copied ? 'Copied!' : 'Copy to clipboard'"
                >
                  {{ copied ? '✓' : '📋' }}
                </button>
              </div>
            </div>

            <div class="warning-box">
              <strong>⚠️ Important:</strong> This is the only time you'll see this key.
              Store it securely - we only save a hash and cannot recover the original key.
            </div>

            <div class="created-key-info">
              <div class="info-row">
                <span class="label">Name:</span>
                <span class="value">{{ createdKey.name }}</span>
              </div>
              <div class="info-row">
                <span class="label">Key Prefix:</span>
                <span class="value"><code>{{ createdKey.key_prefix }}...</code></span>
              </div>
              <div class="info-row">
                <span class="label">Status:</span>
                <span :class="['status-badge', createdKey.enabled ? 'status-enabled' : 'status-disabled']">
                  {{ createdKey.enabled ? 'Enabled' : 'Disabled' }}
                </span>
              </div>
            </div>
          </div>

          <!-- Form state -->
          <div v-else class="form-fields">
            <div class="form-group">
              <label for="name" class="required">Name</label>
              <input
                id="name"
                v-model="formData.name"
                type="text"
                placeholder="e.g., production-api"
                required
                pattern="[a-zA-Z0-9_-]+"
                maxlength="64"
                class="form-input"
              />
              <small class="field-hint">Alphanumeric with dash/underscore, 1-64 characters</small>
            </div>

            <div class="form-group">
              <label for="key" class="required">API Key</label>
              <div class="key-input-group">
                <input
                  id="key"
                  v-model="formData.key"
                  :type="showKey ? 'text' : 'password'"
                  placeholder="Enter your API key (min 16 characters)"
                  required
                  minlength="16"
                  class="form-input"
                />
                <button
                  type="button"
                  @click="showKey = !showKey"
                  class="toggle-visibility-btn"
                  :title="showKey ? 'Hide' : 'Show'"
                >
                  {{ showKey ? '👁️' : '👁️‍🗨️' }}
                </button>
              </div>
              <small class="field-hint">Minimum 16 characters. This key will be hashed and cannot be recovered.</small>
            </div>

            <div class="form-group">
              <label for="description">Description (Optional)</label>
              <textarea
                id="description"
                v-model="formData.description"
                placeholder="Brief description of this API key's purpose"
                rows="3"
                maxlength="255"
                class="form-input"
              />
            </div>

            <div class="form-group">
              <label class="checkbox-label">
                <input
                  v-model="formData.enabled"
                  type="checkbox"
                  class="form-checkbox"
                />
                <span>Enable this API key immediately</span>
              </label>
            </div>
          </div>
        </div>

        <div class="modal-footer">
          <button
            v-if="createdKey"
            type="button"
            @click="$emit('created')"
            class="btn-primary"
          >
            Done
          </button>
          <template v-else>
            <button type="button" @click="$emit('close')" class="btn-secondary">
              Cancel
            </button>
            <button type="submit" class="btn-primary" :disabled="submitting">
              {{ submitting ? 'Creating...' : 'Create API Key' }}
            </button>
          </template>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive } from 'vue'
import { apiKeysApi, type ApiKey } from '@/api/config'

const emit = defineEmits<{
  close: []
  created: []
}>()

const formData = reactive({
  name: '',
  key: '',
  description: '',
  enabled: true,
})

const showKey = ref(false)
const submitting = ref(false)
const error = ref('')
const createdKey = ref<ApiKey | null>(null)
const copied = ref(false)

async function handleSubmit() {
  submitting.value = true
  error.value = ''

  try {
    const created = await apiKeysApi.create(formData)
    createdKey.value = created
  } catch (err: any) {
    console.error('Failed to create API key:', err)
    error.value = err.response?.data?.error?.message || 'Failed to create API key'
  } finally {
    submitting.value = false
  }
}

async function copyToClipboard() {
  try {
    await navigator.clipboard.writeText(formData.key)
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch (err) {
    console.error('Failed to copy to clipboard:', err)
  }
}
</script>

<style scoped>
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
  padding: 1rem;
}

.modal-dialog {
  background: #1a1a1a;
  border: 1px solid #2a2a2a;
  border-radius: 8px;
  max-width: 600px;
  width: 100%;
  max-height: 90vh;
  overflow-y: auto;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5);
}

.modal-header {
  padding: 1.5rem;
  border-bottom: 1px solid #2a2a2a;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.modal-header h3 {
  margin: 0;
  font-size: 1.25rem;
  color: #e5e5e5;
}

.close-btn {
  background: none;
  border: none;
  color: #888;
  font-size: 1.5rem;
  cursor: pointer;
  padding: 0;
  width: 2rem;
  height: 2rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  transition: all 0.2s;
}

.close-btn:hover {
  background: #2a2a2a;
  color: #e5e5e5;
}

.modal-body {
  padding: 1.5rem;
}

.error-message {
  padding: 1rem;
  background: #2c1010;
  border: 1px solid #e74c3c;
  border-radius: 4px;
  color: #e74c3c;
  margin-bottom: 1rem;
}

.form-fields {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.form-group label {
  font-size: 0.875rem;
  font-weight: 600;
  color: #e5e5e5;
}

.form-group label.required::after {
  content: ' *';
  color: #e74c3c;
}

.form-input {
  padding: 0.75rem;
  background: #0a0a0a;
  border: 1px solid #2a2a2a;
  border-radius: 4px;
  color: #e5e5e5;
  font-size: 0.875rem;
  font-family: inherit;
}

.form-input:focus {
  outline: none;
  border-color: #4a9eff;
}

.form-input::placeholder {
  color: #555;
}

textarea.form-input {
  resize: vertical;
  min-height: 80px;
}

.field-hint {
  font-size: 0.75rem;
  color: #666;
}

.key-input-group {
  display: flex;
  gap: 0.5rem;
}

.key-input-group .form-input {
  flex: 1;
}

.toggle-visibility-btn {
  padding: 0.75rem 1rem;
  background: #2a2a2a;
  border: 1px solid #2a2a2a;
  border-radius: 4px;
  cursor: pointer;
  font-size: 1.25rem;
  transition: all 0.2s;
}

.toggle-visibility-btn:hover {
  background: #3a3a3a;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  cursor: pointer;
  user-select: none;
}

.form-checkbox {
  width: 1.25rem;
  height: 1.25rem;
  cursor: pointer;
}

/* Success State */
.success-state {
  text-align: center;
}

.success-icon {
  width: 4rem;
  height: 4rem;
  margin: 0 auto 1rem;
  background: #1a4d2e;
  color: #4ade80;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 2rem;
  font-weight: 700;
}

.success-state h4 {
  color: #e5e5e5;
  margin: 0 0 1.5rem 0;
}

.key-display {
  margin-bottom: 1.5rem;
  text-align: left;
}

.key-display label {
  display: block;
  font-size: 0.875rem;
  font-weight: 600;
  color: #e5e5e5;
  margin-bottom: 0.5rem;
}

.key-box {
  display: flex;
  gap: 0.5rem;
  padding: 1rem;
  background: #0a0a0a;
  border: 1px solid #2a2a2a;
  border-radius: 4px;
}

.key-box code {
  flex: 1;
  font-family: 'Monaco', 'Courier New', monospace;
  font-size: 0.875rem;
  color: #4ade80;
  word-break: break-all;
}

.copy-btn {
  padding: 0.5rem 1rem;
  background: #2a2a2a;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 1.25rem;
  transition: all 0.2s;
}

.copy-btn:hover {
  background: #3a3a3a;
}

.warning-box {
  padding: 1rem;
  background: #4a2d1d;
  border: 1px solid #e07b39;
  border-radius: 4px;
  color: #f0ad4e;
  font-size: 0.875rem;
  margin-bottom: 1.5rem;
  text-align: left;
}

.created-key-info {
  text-align: left;
  background: #0a0a0a;
  border: 1px solid #2a2a2a;
  border-radius: 4px;
  padding: 1rem;
}

.info-row {
  display: flex;
  justify-content: space-between;
  padding: 0.5rem 0;
  border-bottom: 1px solid #222;
}

.info-row:last-child {
  border-bottom: none;
}

.info-row .label {
  font-weight: 600;
  color: #888;
}

.info-row .value {
  color: #e5e5e5;
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

.modal-footer {
  padding: 1.5rem;
  border-top: 1px solid #2a2a2a;
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
}

.btn-primary,
.btn-secondary {
  padding: 0.75rem 1.5rem;
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

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: #2a2a2a;
  color: #e5e5e5;
}

.btn-secondary:hover {
  background: #3a3a3a;
}
</style>
