<template>
  <div v-if="show" class="modal-overlay" @click.self="$emit('close')">
    <div class="modal-dialog">
      <div class="modal-header">
        <h3>Create {{ provider.toUpperCase() }} Instance</h3>
        <button @click="$emit('close')" class="close-btn">✕</button>
      </div>

      <form @submit.prevent="handleSubmit">
        <div class="modal-body">
          <div v-if="error" class="error-message">
            {{ error }}
          </div>

          <div class="form-fields">
            <!-- Instance Name -->
            <div class="form-group">
              <label for="name" class="required">Instance Name</label>
              <input
                id="name"
                v-model="formData.name"
                type="text"
                placeholder="e.g., openai-primary"
                required
                pattern="[a-zA-Z0-9_-]+"
                maxlength="64"
                class="form-input"
              />
              <small class="field-hint">Alphanumeric with dash/underscore, 1-64 characters</small>
            </div>

            <!-- API Key -->
            <div class="form-group">
              <label for="api_key" class="required">API Key</label>
              <div class="key-input-group">
                <input
                  id="api_key"
                  v-model="formData.api_key"
                  :type="showApiKey ? 'text' : 'password'"
                  placeholder="Enter provider API key"
                  required
                  minlength="16"
                  class="form-input"
                />
                <button
                  type="button"
                  @click="showApiKey = !showApiKey"
                  class="toggle-visibility-btn"
                  :title="showApiKey ? 'Hide' : 'Show'"
                >
                  {{ showApiKey ? '👁️' : '👁️‍🗨️' }}
                </button>
              </div>
              <small class="field-hint">API key for {{ provider }} provider</small>
            </div>

            <!-- Base URL -->
            <div class="form-group">
              <label for="base_url" class="required">Base URL</label>
              <input
                id="base_url"
                v-model="formData.base_url"
                type="url"
                :placeholder="getDefaultBaseUrl()"
                required
                class="form-input"
              />
              <small class="field-hint">API endpoint base URL</small>
            </div>

            <!-- Priority -->
            <div class="form-group">
              <label for="priority">Priority</label>
              <input
                id="priority"
                v-model.number="formData.priority"
                type="number"
                placeholder="1"
                min="1"
                max="100"
                class="form-input"
              />
              <small class="field-hint">Lower number = higher priority (default: 1)</small>
            </div>

            <!-- Timeout -->
            <div class="form-group">
              <label for="timeout_seconds">Timeout (seconds)</label>
              <input
                id="timeout_seconds"
                v-model.number="formData.timeout_seconds"
                type="number"
                placeholder="300"
                min="10"
                max="600"
                class="form-input"
              />
              <small class="field-hint">Request timeout in seconds (default: 300)</small>
            </div>

            <!-- Anthropic-specific fields -->
            <template v-if="provider === 'anthropic'">
              <div class="section-divider">
                <span class="section-title">Anthropic-Specific Settings</span>
              </div>

              <div class="form-group">
                <label for="api_version">API Version</label>
                <input
                  id="api_version"
                  v-model="anthropicConfig.api_version"
                  type="text"
                  placeholder="2023-06-01"
                  class="form-input"
                />
                <small class="field-hint">Anthropic API version (default: 2023-06-01)</small>
              </div>

              <div class="form-group">
                <label class="checkbox-label">
                  <input
                    v-model="anthropicConfig.auto_cache_system"
                    type="checkbox"
                    class="form-checkbox"
                  />
                  <span>Auto-cache system prompts</span>
                </label>
                <small class="field-hint">Automatically enable prompt caching for system messages</small>
              </div>

              <div class="form-group">
                <label for="min_system_tokens">Min System Tokens for Caching</label>
                <input
                  id="min_system_tokens"
                  v-model.number="anthropicConfig.min_system_tokens"
                  type="number"
                  placeholder="1024"
                  min="0"
                  max="10000"
                  class="form-input"
                />
                <small class="field-hint">Minimum tokens required to enable system prompt caching</small>
              </div>
            </template>

            <!-- Description -->
            <div class="form-group">
              <label for="description">Description (Optional)</label>
              <textarea
                id="description"
                v-model="formData.description"
                placeholder="Brief description of this instance"
                rows="3"
                maxlength="255"
                class="form-input"
              />
            </div>

            <!-- Enabled -->
            <div class="form-group">
              <label class="checkbox-label">
                <input
                  v-model="formData.enabled"
                  type="checkbox"
                  class="form-checkbox"
                />
                <span>Enable this instance immediately</span>
              </label>
            </div>
          </div>
        </div>

        <div class="modal-footer">
          <button type="button" @click="$emit('close')" class="btn-secondary">
            Cancel
          </button>
          <button type="submit" class="btn-primary" :disabled="submitting">
            {{ submitting ? 'Creating...' : 'Create Instance' }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, watch } from 'vue'
import { providerInstancesApi } from '@/api/config'

const props = defineProps<{
  show: boolean
  provider: string
}>()

const emit = defineEmits<{
  close: []
  created: []
}>()

const formData = reactive({
  name: '',
  api_key: '',
  base_url: '',
  priority: 1,
  timeout_seconds: 300,
  description: '',
  enabled: true,
})

const anthropicConfig = reactive({
  api_version: '2023-06-01',
  auto_cache_system: true,
  min_system_tokens: 1024,
})

const showApiKey = ref(false)
const submitting = ref(false)
const error = ref('')

// Reset form when modal is opened
watch(() => props.show, (newVal) => {
  if (newVal) {
    formData.name = ''
    formData.api_key = ''
    formData.base_url = getDefaultBaseUrl()
    formData.priority = 1
    formData.timeout_seconds = 300
    formData.description = ''
    formData.enabled = true

    // Reset Anthropic config
    anthropicConfig.api_version = '2023-06-01'
    anthropicConfig.auto_cache_system = true
    anthropicConfig.min_system_tokens = 1024

    showApiKey.value = false
    error.value = ''
  }
})

function getDefaultBaseUrl(): string {
  switch (props.provider) {
    case 'openai':
      return 'https://api.openai.com/v1'
    case 'anthropic':
      return 'https://api.anthropic.com/v1'
    case 'gemini':
      return 'https://generativelanguage.googleapis.com/v1beta'
    default:
      return ''
  }
}

async function handleSubmit() {
  submitting.value = true
  error.value = ''

  try {
    // Build request payload
    const payload: any = {
      name: formData.name,
      api_key: formData.api_key,
      base_url: formData.base_url,
      enabled: formData.enabled,
      priority: formData.priority || undefined,
      timeout_seconds: formData.timeout_seconds || undefined,
      description: formData.description || undefined,
    }

    // Add Anthropic extra_config if applicable
    if (props.provider === 'anthropic') {
      payload.extra_config = {
        api_version: anthropicConfig.api_version,
        cache: {
          auto_cache_system: anthropicConfig.auto_cache_system,
          min_system_tokens: anthropicConfig.min_system_tokens,
          auto_cache_tools: false,
        },
      }
    }

    await providerInstancesApi.create(props.provider, payload)
    emit('created')
  } catch (err: any) {
    console.error('Failed to create provider instance:', err)
    error.value = err.response?.data?.error?.message || 'Failed to create provider instance'
  } finally {
    submitting.value = false
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

.section-divider {
  margin: 1rem 0;
  padding-top: 1rem;
  border-top: 1px solid #2a2a2a;
}

.section-title {
  font-size: 0.875rem;
  font-weight: 700;
  color: #4a9eff;
  text-transform: uppercase;
  letter-spacing: 0.05em;
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
