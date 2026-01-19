<template>
  <MainLayout>
    <RouterView />
  </MainLayout>
</template>

<script setup lang="ts">
import { RouterView } from 'vue-router'
import MainLayout from '@/components/common/MainLayout.vue'
</script>

<style>
#app {
  min-height: 100vh;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
}
</style>
