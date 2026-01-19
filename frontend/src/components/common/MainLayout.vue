<template>
  <div class="min-h-screen">
    <!-- Header -->
    <AppHeader />

    <!-- Main Content -->
    <main class="container mx-auto px-4 py-8">
      <slot />
    </main>
  </div>
</template>

<script setup lang="ts">
import AppHeader from './AppHeader.vue'
</script>
