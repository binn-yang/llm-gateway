<template>
  <div class="glass-strong rounded-xl p-6">
    <div class="flex items-center justify-between">
      <div>
        <p class="text-sm text-gray-300 mb-1">{{ title }}</p>
        <p class="text-2xl font-bold text-white">{{ formattedValue }}</p>
      </div>
      <div
        class="w-12 h-12 rounded-lg flex items-center justify-center"
        :style="{ background: colorBg }"
      >
        <el-icon :size="24" :color="color">
          <component :is="icon" />
        </el-icon>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  title: string
  value: string | number
  icon: string
  color: 'primary' | 'secondary' | 'accent' | 'danger'
}

const props = defineProps<Props>()

const formattedValue = computed(() => {
  return typeof props.value === 'string' ? props.value : props.value.toString()
})

const colorBg = computed(() => {
  const colors = {
    primary: 'rgba(102, 126, 234, 0.3)',
    secondary: 'rgba(118, 75, 162, 0.3)',
    accent: 'rgba(240, 147, 251, 0.3)',
    danger: 'rgba(239, 68, 68, 0.3)',
  }
  return colors[props.color]
})

const color = computed(() => {
  const colors = {
    primary: '#667eea',
    secondary: '#764ba2',
    accent: '#f093fb',
    danger: '#ef4444',
  }
  return colors[props.color]
})
</script>
