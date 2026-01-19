<template>
  <div class="space-y-6">
    <!-- Header -->
    <h1 class="text-2xl font-bold text-white">Settings</h1>

    <!-- System Info -->
    <el-card class="glass-strong" shadow="hover">
      <template #header>
        <span class="text-white font-bold">System Information</span>
      </template>

      <el-descriptions :column="2" border v-loading="loading">
        <el-descriptions-item label="Version">
          <el-text class="text-white">0.3.0</el-text>
        </el-descriptions-item>
        <el-descriptions-item label="Environment">
          <el-text class="text-white">{{ environment }}</el-text>
        </el-descriptions-item>
        <el-descriptions-item label="Backend URL">
          <el-text class="text-white">{{ apiBaseUrl }}</el-text>
        </el-descriptions-item>
        <el-descriptions-item label="Frontend URL">
          <el-text class="text-white">{{ frontendUrl }}</el-text>
        </el-descriptions-item>
      </el-descriptions>
    </el-card>

    <!-- Configuration -->
    <el-card class="glass-strong" shadow="hover">
      <template #header>
        <div class="flex justify-between items-center">
          <span class="text-white font-bold">Configuration Summary</span>
          <el-button size="small" @click="fetchConfig" :loading="loading">
            <el-icon><Refresh /></el-icon>
            Refresh
          </el-button>
        </div>
      </template>

      <div v-if="error">
        <el-alert type="error" :closable="false">
          Failed to load configuration: {{ error.message }}
        </el-alert>
      </div>

      <div v-else-if="config">
        <el-descriptions :column="1" border>
          <el-descriptions-item label="Server Host">
            <el-text class="text-white">{{ config.server.host }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Server Port">
            <el-text class="text-white">{{ config.server.port }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Log Level">
            <el-text class="text-white">{{ config.server.log_level }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Default Provider">
            <el-text class="text-white">{{ config.routing.default_provider || 'None' }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Providers">
            <div class="text-white space-y-2">
              <div v-for="(count, provider) in config.providers" :key="provider" class="flex justify-between">
                <span class="capitalize">{{ provider }}:</span>
                <span>{{ count }} instance(s)</span>
              </div>
            </div>
          </el-descriptions-item>
        </el-descriptions>
      </div>
    </el-card>

    <!-- Links -->
    <el-card class="glass-strong" shadow="hover">
      <template #header>
        <span class="text-white font-bold">Quick Links</span>
      </template>

      <div class="space-y-3">
        <div>
          <el-text class="text-gray-400">API Documentation:</el-text>
          <div class="mt-1">
            <el-link href="/api/dashboard/docs" target="_blank" class="text-white">
              /api/dashboard/docs
            </el-link>
          </div>
        </div>
        <div>
          <el-text class="text-gray-400">Prometheus Metrics:</el-text>
          <div class="mt-1">
            <el-link href="/metrics" target="_blank" class="text-white">
              /metrics
            </el-link>
          </div>
        </div>
        <div>
          <el-text class="text-gray-400">Health Check:</el-text>
          <div class="mt-1">
            <el-link href="/health" target="_blank" class="text-white">
              /health
            </el-link>
          </div>
        </div>
      </div>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { dashboardApi, type ConfigSummary } from '@/api/dashboard'

const loading = ref(false)
const error = ref<Error | null>(null)
const config = ref<ConfigSummary | null>(null)

const environment = computed(() => {
  return import.meta.env.MODE
})

const apiBaseUrl = computed(() => {
  return import.meta.env.VITE_API_BASE_URL || '/api'
})

const frontendUrl = computed(() => {
  return typeof window !== 'undefined' ? window.location.origin : 'unknown'
})

async function fetchConfig() {
  loading.value = true
  error.value = null

  try {
    config.value = await dashboardApi.getConfig()
  } catch (err) {
    error.value = err as Error
    console.error('Failed to fetch config:', err)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  fetchConfig()
})
</script>
