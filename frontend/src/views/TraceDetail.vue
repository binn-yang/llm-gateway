<template>
  <div class="space-y-6">
    <!-- Header -->
    <div class="flex items-center gap-4">
      <el-button @click="goBack" circle>
        <el-icon><ArrowLeft /></el-icon>
      </el-button>
      <h1 class="text-2xl font-bold text-white">Trace Detail</h1>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="glass-strong rounded-xl p-12 flex justify-center">
      <el-icon class="is-loading text-white text-6xl"><Loading /></el-icon>
    </div>

    <!-- Error -->
    <div v-else-if="error" class="glass-strong rounded-xl p-6">
      <el-alert type="error" :closable="false">
        Failed to load trace: {{ error.message }}
      </el-alert>
    </div>

    <!-- Trace Content -->
    <div v-else-if="trace" class="space-y-6">
      <!-- Trace Summary -->
      <el-card class="glass-strong" shadow="hover">
        <template #header>
          <span class="text-white font-bold">Request Summary</span>
        </template>

        <el-descriptions :column="2" border>
          <el-descriptions-item label="Request ID">
            <el-text class="text-white">{{ trace.request_id }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Status">
            <el-tag :type="trace.status === 'success' ? 'success' : 'danger'">
              {{ trace.status }}
            </el-tag>
          </el-descriptions-item>
          <el-descriptions-item label="Model">
            <el-text class="text-white">{{ trace.model }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Provider">
            <el-text class="text-white capitalize">{{ trace.provider }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Duration">
            <el-text class="text-white">{{ formatDuration(trace.duration_ms) }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Start Time">
            <el-text class="text-white">{{ formatTimestamp(trace.start_time) }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Input Tokens" v-if="trace.input_tokens">
            <el-text class="text-white">{{ trace.input_tokens }}</el-text>
          </el-descriptions-item>
          <el-descriptions-item label="Output Tokens" v-if="trace.output_tokens">
            <el-text class="text-white">{{ trace.output_tokens }}</el-text>
          </el-descriptions-item>
        </el-descriptions>
      </el-card>

      <!-- Span Tree -->
      <TraceTree :spans="trace.spans" />

      <!-- Timeline -->
      <TraceTimeline :spans="trace.spans" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, Loading } from '@element-plus/icons-vue'
import { useTracesStore } from '@/stores/traces'
import type { Trace } from '@/api/traces'
import dayjs from 'dayjs'
import TraceTree from '@/components/trace/TraceTree.vue'
import TraceTimeline from '@/components/trace/TraceTimeline.vue'

const route = useRoute()
const router = useRouter()
const tracesStore = useTracesStore()

const trace = ref<Trace | null>(null)
const loading = ref(false)
const error = ref<Error | null>(null)

async function fetchTrace() {
  const requestId = route.params.requestId as string
  if (!requestId) return

  loading.value = true
  error.value = null

  try {
    trace.value = await tracesStore.fetchTrace(requestId)
  } catch (err) {
    error.value = err as Error
  } finally {
    loading.value = false
  }
}

function goBack() {
  router.push('/traces')
}

function formatDuration(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)}μs`
  if (ms < 1000) return `${ms.toFixed(2)}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function formatTimestamp(timestamp: string): string {
  return dayjs(timestamp).format('YYYY-MM-DD HH:mm:ss.SSS')
}

onMounted(() => {
  fetchTrace()
})
</script>
