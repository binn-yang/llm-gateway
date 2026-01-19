import axios from 'axios'

const apiClient = axios.create({
  baseURL: import.meta.env.VITE_API_BASE_URL || '/api',
  timeout: 30000,
  headers: {
    'Content-Type': 'application/json',
  },
})

// Request interceptor
apiClient.interceptors.request.use(
  (config) => {
    return config
  },
  (error) => {
    return Promise.reject(error)
  }
)

// Response interceptor
apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response) {
      // Server responded with error status
      const message = error.response.data?.message || 'Request failed'
      console.error('API error:', message)
    } else if (error.request) {
      // Request made but no response
      console.error('Network error - no response received')
    } else {
      // Error setting up request
      console.error('Request setup error:', error.message)
    }
    return Promise.reject(error)
  }
)

export default apiClient
