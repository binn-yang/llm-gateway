<template>
  <div class="glass-strong rounded-xl p-6">
    <h1 class="text-2xl font-bold text-white mb-4">
      Trace Detail: {{ requestId }}
    </h1>
    <p class="text-gray-300">Trace detail view coming soon...</p>
    <p class="text-sm text-gray-400 mt-2">
      This page will display detailed information about a specific request trace.
    </p>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'

const route = useRoute()
const requestId = computed(() => route.params.requestId as string)
</script>
