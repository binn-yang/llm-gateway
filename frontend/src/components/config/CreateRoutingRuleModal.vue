<template>
  <div v-if="show" class="modal-overlay" @click.self="$emit('close')">
    <div class="modal-dialog">
      <div class="modal-header">
        <h3>Create Routing Rule</h3>
        <button @click="$emit('close')" class="close-btn">✕</button>
      </div>

      <form @submit.prevent="handleSubmit">
        <div class="modal-body">
          <div v-if="error" class="error-message">
            {{ error }}
          </div>

          <div class="form-fields">
            <div class="form-group">
              <label for="prefix" class="required">Model Prefix</label>
              <input
                id="prefix"
                v-model="formData.prefix"
                type="text"
                placeholder="e.g., gpt-, claude-, gemini-"
                required
                pattern="[a-zA-Z0-9_.-]+"
                maxlength="64"
                class="form-input"
              />
              <small class="field-hint">Prefix to match model names (alphanumeric with dash/dot/underscore, 1-64 characters)</small>
            </div>

            <div class="form-group">
              <label for="provider" class="required">Provider</label>
              <select
                id="provider"
                v-model="formData.provider"
                required
                class="form-input"
              >
                <option value="">-- Select Provider --</option>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
                <option value="gemini">Gemini</option>
              </select>
              <small class="field-hint">Target provider for this routing rule</small>
            </div>

            <div class="form-group">
              <label for="priority">Priority</label>
              <input
                id="priority"
                v-model.number="formData.priority"
                type="number"
                placeholder="100"
                min="1"
                max="1000"
                class="form-input"
              />
              <small class="field-hint">Lower number = higher priority (default: 100)</small>
            </div>

            <div class="form-group">
              <label for="description">Description (Optional)</label>
              <textarea
                id="description"
                v-model="formData.description"
                placeholder="Brief description of this routing rule"
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
                <span>Enable this routing rule immediately</span>
              </label>
            </div>
          </div>
        </div>

        <div class="modal-footer">
          <button type="button" @click="$emit('close')" class="btn-secondary">
            Cancel
          </button>
          <button type="submit" class="btn-primary" :disabled="submitting">
            {{ submitting ? 'Creating...' : 'Create Rule' }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, watch } from 'vue'
import { routingRulesApi } from '@/api/config'

const props = defineProps<{
  show: boolean
}>()

const emit = defineEmits<{
  close: []
  created: []
}>()

const formData = reactive({
  prefix: '',
  provider: '',
  priority: 100,
  description: '',
  enabled: true,
})

const submitting = ref(false)
const error = ref('')

// Reset form when modal is opened
watch(() => props.show, (newVal) => {
  if (newVal) {
    formData.prefix = ''
    formData.provider = ''
    formData.priority = 100
    formData.description = ''
    formData.enabled = true
    error.value = ''
  }
})

async function handleSubmit() {
  submitting.value = true
  error.value = ''

  try {
    await routingRulesApi.create({
      prefix: formData.prefix,
      provider: formData.provider,
      priority: formData.priority || undefined,
      description: formData.description || undefined,
      enabled: formData.enabled,
    })
    emit('created')
  } catch (err: any) {
    console.error('Failed to create routing rule:', err)
    error.value = err.response?.data?.error?.message || 'Failed to create routing rule'
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

select.form-input {
  cursor: pointer;
}

textarea.form-input {
  resize: vertical;
  min-height: 80px;
}

.field-hint {
  font-size: 0.75rem;
  color: #666;
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
