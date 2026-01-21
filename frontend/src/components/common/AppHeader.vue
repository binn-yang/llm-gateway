<template>
  <header class="glass-strong sticky top-0 z-50">
    <div class="container mx-auto px-4 py-4">
      <div class="flex items-center justify-between">
        <!-- Logo and Title -->
        <div class="flex items-center space-x-4">
          <h1 class="text-2xl font-bold text-white">
            LLM Gateway
          </h1>
          <span class="text-sm text-gray-300">
            Dashboard
          </span>
        </div>

        <!-- Navigation -->
        <nav class="flex space-x-1">
          <RouterLink
            v-for="route in routes"
            :key="route.name"
            :to="route.path"
            class="px-4 py-2 rounded-lg text-white hover:bg-white/10 transition-colors"
            active-class="bg-white/20"
          >
            {{ route.meta?.title || route.name }}
          </RouterLink>
        </nav>

        <!-- Actions -->
        <div class="flex items-center space-x-4">
          <span class="text-sm text-gray-300" v-if="lastFetchTime">
            Last updated: {{ formatTime(lastFetchTime) }}
          </span>
        </div>
      </div>
    </div>
  </header>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { RouterLink } from 'vue-router'

interface Route {
  name: string
  path: string
  meta?: { title?: string }
}

const routes: Route[] = [
  { name: 'Dashboard', path: '/', meta: { title: 'Dashboard' } },
  { name: 'Logs', path: '/logs', meta: { title: 'Logs' } },
  { name: 'Settings', path: '/settings', meta: { title: 'Settings' } },
]

const lastFetchTime = ref<Date | null>(null)

function formatTime(date: Date): string {
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < 60000) {
    return `${Math.floor(diff / 1000)}s ago`
  } else if (diff < 3600000) {
    return `${Math.floor(diff / 60000)}m ago`
  } else {
    return date.toLocaleTimeString()
  }
}
</script>
