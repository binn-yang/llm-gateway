import { createRouter, createWebHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'Dashboard',
    component: () => import('@/views/Dashboard.vue'),
    meta: { title: 'Dashboard' },
  },
  {
    path: '/logs',
    name: 'Logs',
    component: () => import('@/views/Logs.vue'),
    meta: { title: 'Logs' },
  },
  {
    path: '/traces',
    name: 'Traces',
    component: () => import('@/views/Traces.vue'),
    meta: { title: 'Traces' },
  },
  {
    path: '/traces/:requestId',
    name: 'TraceDetail',
    component: () => import('@/views/TraceDetail.vue'),
    meta: { title: 'Trace Detail' },
  },
  {
    path: '/settings',
    name: 'Settings',
    component: () => import('@/views/Settings.vue'),
    meta: { title: 'Settings' },
  },
  {
    path: '/:pathMatch(.*)*',
    redirect: '/',
  },
]

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes,
})

// Update page title
router.beforeEach((to) => {
  const title = to.meta.title as string | undefined
  if (title) {
    document.title = `${title} - LLM Gateway`
  }
})

export default router
