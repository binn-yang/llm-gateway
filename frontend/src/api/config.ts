/**
 * Configuration Management API Client
 *
 * Provides TypeScript interfaces and API functions for managing gateway configuration:
 * - API Keys
 * - Routing Rules
 * - Provider Instances
 */

import apiClient from './client'

// ============================================================================
// TypeScript Interfaces
// ============================================================================

// API Keys
export interface ApiKey {
  id: number
  name: string
  key_prefix: string
  enabled: boolean
  description?: string
  created_at: number
  updated_at: number
  last_used_at?: number
}

export interface CreateApiKeyRequest {
  name: string
  key: string  // Full key (only provided on creation)
  enabled?: boolean
  description?: string
}

export interface UpdateApiKeyRequest {
  enabled?: boolean
  description?: string
}

// Routing Rules
export interface RoutingRule {
  id: number
  prefix: string
  provider: string
  priority: number
  enabled: boolean
  description?: string
  created_at: number
  updated_at: number
}

export interface CreateRoutingRuleRequest {
  prefix: string
  provider: string
  priority?: number
  enabled?: boolean
  description?: string
}

export interface UpdateRoutingRuleRequest {
  provider?: string
  priority?: number
  enabled?: boolean
  description?: string
}

// Routing Global Config
export interface RoutingConfig {
  default_provider?: string
  discovery_enabled: boolean
  discovery_cache_ttl_seconds: number
  discovery_refresh_on_startup: boolean
  discovery_providers_with_listing: string[]
}

export interface UpdateRoutingConfigRequest {
  default_provider?: string
  discovery_enabled?: boolean
  discovery_cache_ttl_seconds?: number
  discovery_refresh_on_startup?: boolean
  discovery_providers_with_listing?: string[]
}

// Provider Instances
export interface ProviderInstance {
  id: number
  provider: string
  name: string
  enabled: boolean
  base_url: string
  timeout_seconds: number
  priority: number
  weight: number
  failure_timeout_seconds: number
  extra_config?: any  // JSON object
  description?: string
  created_at: number
  updated_at: number
  health_status?: string
}

export interface CreateProviderInstanceRequest {
  name: string
  api_key: string
  base_url: string
  enabled?: boolean
  timeout_seconds?: number
  priority?: number
  weight?: number
  failure_timeout_seconds?: number
  extra_config?: any
  description?: string
}

export interface UpdateProviderInstanceRequest {
  enabled?: boolean
  api_key?: string
  base_url?: string
  timeout_seconds?: number
  priority?: number
  weight?: number
  failure_timeout_seconds?: number
  extra_config?: any
  description?: string
}

// Anthropic-specific extra_config
export interface AnthropicExtraConfig {
  api_version?: string
  cache?: {
    auto_cache_system?: boolean
    min_system_tokens?: number
    auto_cache_tools?: boolean
  }
}

// ============================================================================
// API Functions
// ============================================================================

/**
 * API Keys Management
 */
export const apiKeysApi = {
  /**
   * List all API keys
   */
  list: async (): Promise<ApiKey[]> => {
    const response = await apiClient.get<ApiKey[]>('/config/api-keys')
    return response.data
  },

  /**
   * Create a new API key
   */
  create: async (data: CreateApiKeyRequest): Promise<ApiKey> => {
    const response = await apiClient.post<ApiKey>('/config/api-keys', data)
    return response.data
  },

  /**
   * Update an API key
   */
  update: async (name: string, data: UpdateApiKeyRequest): Promise<ApiKey> => {
    const response = await apiClient.put<ApiKey>(`/config/api-keys/${name}`, data)
    return response.data
  },

  /**
   * Delete an API key (soft delete)
   */
  delete: async (name: string): Promise<void> => {
    await apiClient.delete(`/config/api-keys/${name}`)
  },
}

/**
 * Routing Rules Management
 */
export const routingRulesApi = {
  /**
   * List all routing rules
   */
  list: async (): Promise<RoutingRule[]> => {
    const response = await apiClient.get<RoutingRule[]>('/config/routing/rules')
    return response.data
  },

  /**
   * Create a new routing rule
   */
  create: async (data: CreateRoutingRuleRequest): Promise<RoutingRule> => {
    const response = await apiClient.post<RoutingRule>('/config/routing/rules', data)
    return response.data
  },

  /**
   * Update a routing rule
   */
  update: async (id: number, data: UpdateRoutingRuleRequest): Promise<RoutingRule> => {
    const response = await apiClient.put<RoutingRule>(`/config/routing/rules/${id}`, data)
    return response.data
  },

  /**
   * Delete a routing rule (soft delete)
   */
  delete: async (id: number): Promise<void> => {
    await apiClient.delete(`/config/routing/rules/${id}`)
  },
}

/**
 * Routing Global Config Management
 */
export const routingConfigApi = {
  /**
   * Get global routing configuration
   */
  get: async (): Promise<RoutingConfig> => {
    const response = await apiClient.get<RoutingConfig>('/config/routing/global')
    return response.data
  },

  /**
   * Update global routing configuration
   */
  update: async (data: UpdateRoutingConfigRequest): Promise<RoutingConfig> => {
    const response = await apiClient.put<RoutingConfig>('/config/routing/global', data)
    return response.data
  },
}

/**
 * Provider Instances Management
 */
export const providerInstancesApi = {
  /**
   * List provider instances for a specific provider
   */
  list: async (provider: string): Promise<ProviderInstance[]> => {
    const response = await apiClient.get<ProviderInstance[]>(`/config/providers/${provider}/instances`)
    return response.data
  },

  /**
   * Create a new provider instance
   */
  create: async (provider: string, data: CreateProviderInstanceRequest): Promise<ProviderInstance> => {
    const response = await apiClient.post<ProviderInstance>(`/config/providers/${provider}/instances`, data)
    return response.data
  },

  /**
   * Update a provider instance
   */
  update: async (
    provider: string,
    name: string,
    data: UpdateProviderInstanceRequest
  ): Promise<ProviderInstance> => {
    const response = await apiClient.put<ProviderInstance>(
      `/config/providers/${provider}/instances/${name}`,
      data
    )
    return response.data
  },

  /**
   * Delete a provider instance (soft delete)
   */
  delete: async (provider: string, name: string): Promise<void> => {
    await apiClient.delete(`/config/providers/${provider}/instances/${name}`)
  },
}

/**
 * Utility: Format timestamp to readable date
 */
export const formatDate = (timestamp?: number): string => {
  if (!timestamp) return 'Never'
  return new Date(timestamp).toLocaleString()
}

/**
 * Utility: Format timestamp to relative time
 */
export const formatRelativeTime = (timestamp?: number): string => {
  if (!timestamp) return 'Never'

  const now = Date.now()
  const diff = now - timestamp
  const seconds = Math.floor(diff / 1000)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (days > 0) return `${days} day${days > 1 ? 's' : ''} ago`
  if (hours > 0) return `${hours} hour${hours > 1 ? 's' : ''} ago`
  if (minutes > 0) return `${minutes} minute${minutes > 1 ? 's' : ''} ago`
  if (seconds > 0) return `${seconds} second${seconds > 1 ? 's' : ''} ago`
  return 'Just now'
}
