import { ref, onUnmounted } from 'vue'

export interface PollingOptions<T> {
  /**
   * Function to call on each poll
   */
  fn: () => Promise<T>

  /**
   * Polling interval in milliseconds
   * @default 5000
   */
  interval?: number

  /**
   * Whether to execute immediately on start
   * @default true
   */
  immediate?: boolean

  /**
   * Whether to start polling automatically
   * @default false
   */
  autoStart?: boolean
}

// Export the options as a type for external use
export type UsePollingOptions<T> = PollingOptions<T>

export function usePolling<T>(options: PollingOptions<T>) {
  const {
    fn,
    interval = 5000,
    immediate = true,
    autoStart = false,
  } = options

  const data = ref<T | null>(null)
  const isLoading = ref(false)
  const error = ref<Error | null>(null)
  const isPolling = ref(false)
  let timer: number | null = null
  let pollCount = 0

  async function poll() {
    isLoading.value = true
    error.value = null
    try {
      data.value = await fn()
      pollCount++
    } catch (e) {
      error.value = e as Error
      console.error('Polling error:', e)
    } finally {
      isLoading.value = false
    }
  }

  function start() {
    if (isPolling.value) {
      return // Already polling
    }

    isPolling.value = true

    if (immediate) {
      poll()
    }

    timer = window.setInterval(() => {
      poll()
    }, interval)
  }

  function stop() {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
    isPolling.value = false
  }

  function restart() {
    stop()
    start()
  }

  // Auto-start if enabled
  if (autoStart) {
    start()
  }

  // Cleanup on unmount
  onUnmounted(() => {
    stop()
  })

  return {
    data,
    isLoading,
    error,
    isPolling,
    pollCount,
    start,
    stop,
    restart,
  } as const
}
