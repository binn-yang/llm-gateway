import { createApp } from 'vue'
import { createPinia } from 'pinia'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import Chart from 'chart.js/auto'

import App from './App.vue'
import router from './router'

// Import Tailwind CSS
import './assets/styles/main.css'

// Make Chart.js available globally
if (typeof window !== 'undefined') {
  (window as any).Chart = Chart
}

const app = createApp(App)
const pinia = createPinia()

// Register Element Plus icons
for (const [key, component] of Object.entries(ElementPlusIconsVue)) {
  app.component(key, component)
}

app.use(pinia)
app.use(router)
app.use(ElementPlus)

app.mount('#app')
