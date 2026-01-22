/// Configuration Management API
///
/// This module provides REST API endpoints for managing gateway configuration:
/// - API Keys CRUD
/// - Routing Rules CRUD
/// - Provider Instances CRUD
/// - Configuration hot reload

use crate::{
    config::Config,
    error::AppError,
    load_balancer::LoadBalancer,
    router::Provider,
    server::build_load_balancers,
};
use arc_swap::ArcSwap;
use axum::{
    extract::{Path, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{collections::HashMap, sync::Arc};

/// State for config API handlers
#[derive(Clone)]
pub struct ConfigApiState {
    pub db_pool: SqlitePool,
    pub config: Arc<ArcSwap<Config>>,
    pub load_balancers: Arc<ArcSwap<HashMap<Provider, Arc<LoadBalancer>>>>,
}

/// Create router for config API
pub fn create_config_router(state: ConfigApiState) -> Router {
    Router::new()
        // API Keys
        .route("/api-keys", get(list_api_keys).post(create_api_key))
        .route(
            "/api-keys/:name",
            put(update_api_key).delete(delete_api_key),
        )
        // Routing Rules
        .route("/routing/rules", get(list_routing_rules).post(create_routing_rule))
        .route(
            "/routing/rules/:id",
            put(update_routing_rule).delete(delete_routing_rule),
        )
        // Routing Global Config
        .route("/routing/global", get(get_routing_config).put(update_routing_config))
        // Provider Instances
        .route(
            "/providers/:provider/instances",
            get(list_provider_instances).post(create_provider_instance),
        )
        .route(
            "/providers/:provider/instances/:name",
            put(update_provider_instance).delete(delete_provider_instance),
        )
        .with_state(state)
}

// ============================================================================
// API Keys Endpoints
// ============================================================================

#[derive(Debug, Serialize)]
struct ApiKeyResponse {
    id: i64,
    name: String,
    key_prefix: String,
    enabled: bool,
    description: Option<String>,
    created_at: i64,
    updated_at: i64,
    last_used_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    key: String, // Full key (only provided on creation)
    #[serde(default = "default_true")]
    enabled: bool,
    description: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct UpdateApiKeyRequest {
    enabled: Option<bool>,
    description: Option<String>,
}

/// GET /api/config/api-keys
async fn list_api_keys(
    State(state): State<ConfigApiState>,
) -> Result<Json<Vec<ApiKeyResponse>>, AppError> {
    #[derive(sqlx::FromRow)]
    struct ApiKeyRow {
        id: i64,
        name: String,
        key_prefix: String,
        enabled: i64,
        description: Option<String>,
        created_at: i64,
        updated_at: i64,
        last_used_at: Option<i64>,
    }

    let rows = sqlx::query_as::<_, ApiKeyRow>(
        r#"
        SELECT id, name, key_prefix, enabled, description, created_at, updated_at, last_used_at
        FROM api_keys
        WHERE deleted_at IS NULL
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let api_keys = rows
        .into_iter()
        .map(|row| ApiKeyResponse {
            id: row.id,
            name: row.name,
            key_prefix: row.key_prefix,
            enabled: row.enabled != 0,
            description: row.description,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_used_at: row.last_used_at,
        })
        .collect();

    Ok(Json(api_keys))
}

/// POST /api/config/api-keys
async fn create_api_key(
    State(state): State<ConfigApiState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>, AppError> {
    // Validate name (alphanumeric + dash/underscore, 1-64 chars)
    if !req
        .name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        || req.name.is_empty()
        || req.name.len() > 64
    {
        return Err(AppError::ConfigError(
            "Invalid name: must be alphanumeric with dash/underscore, 1-64 chars".to_string(),
        ));
    }

    // Validate key format (basic check)
    if req.key.len() < 16 {
        return Err(AppError::ConfigError(
            "Invalid key: must be at least 16 characters".to_string(),
        ));
    }

    // Compute SHA256 hash
    let key_hash = format!("{:x}", Sha256::digest(req.key.as_bytes()));

    // Extract prefix (first 8 chars)
    let key_prefix = req.key.chars().take(8).collect::<String>();

    // Insert into database
    let now = chrono::Utc::now().timestamp_millis();
    let result = sqlx::query(
        r#"
        INSERT INTO api_keys (key_hash, key_prefix, name, enabled, description, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&key_hash)
    .bind(&key_prefix)
    .bind(&req.name)
    .bind(if req.enabled { 1 } else { 0 })
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::ConfigError(format!("API key name '{}' already exists", req.name))
        } else {
            AppError::InternalError(format!("Database error: {}", e))
        }
    })?;

    let id = result.last_insert_rowid();

    // Reload configuration
    reload_config(&state).await?;

    Ok(Json(ApiKeyResponse {
        id,
        name: req.name,
        key_prefix,
        enabled: req.enabled,
        description: req.description,
        created_at: now,
        updated_at: now,
        last_used_at: None,
    }))
}

/// PUT /api/config/api-keys/:name
async fn update_api_key(
    State(state): State<ConfigApiState>,
    Path(name): Path<String>,
    Json(req): Json<UpdateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>, AppError> {
    let now = chrono::Utc::now().timestamp_millis();

    // Build dynamic UPDATE query
    let mut updates = Vec::new();
    let mut query = String::from("UPDATE api_keys SET updated_at = ?");
    let mut bind_count = 2; // updated_at + name

    if req.enabled.is_some() {
        updates.push(format!("enabled = ?{}", bind_count));
        bind_count += 1;
    }
    if req.description.is_some() {
        updates.push(format!("description = ?{}", bind_count));
    }

    if !updates.is_empty() {
        query.push_str(", ");
        query.push_str(&updates.join(", "));
    }
    query.push_str(" WHERE name = ? AND deleted_at IS NULL");

    let mut q = sqlx::query(&query).bind(now);

    if let Some(enabled) = req.enabled {
        q = q.bind(if enabled { 1 } else { 0 });
    }
    if let Some(desc) = &req.description {
        q = q.bind(desc);
    }
    q = q.bind(&name);

    let result = q.execute(&state.db_pool).await.map_err(|e| {
        AppError::InternalError(format!("Database error: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(AppError::ConfigError(format!(
            "API key '{}' not found",
            name
        )));
    }

    // Reload configuration
    reload_config(&state).await?;

    // Fetch updated record
    #[derive(sqlx::FromRow)]
    struct ApiKeyRow {
        id: i64,
        name: String,
        key_prefix: String,
        enabled: i64,
        description: Option<String>,
        created_at: i64,
        updated_at: i64,
        last_used_at: Option<i64>,
    }

    let row = sqlx::query_as::<_, ApiKeyRow>(
        r#"
        SELECT id, name, key_prefix, enabled, description, created_at, updated_at, last_used_at
        FROM api_keys
        WHERE name = ? AND deleted_at IS NULL
        "#,
    )
    .bind(&name)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    Ok(Json(ApiKeyResponse {
        id: row.id,
        name: row.name,
        key_prefix: row.key_prefix,
        enabled: row.enabled != 0,
        description: row.description,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_used_at: row.last_used_at,
    }))
}

/// DELETE /api/config/api-keys/:name
async fn delete_api_key(
    State(state): State<ConfigApiState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let now = chrono::Utc::now().timestamp_millis();

    let result = sqlx::query(
        r#"
        UPDATE api_keys
        SET deleted_at = ?
        WHERE name = ? AND deleted_at IS NULL
        "#,
    )
    .bind(now)
    .bind(&name)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::ConfigError(format!(
            "API key '{}' not found",
            name
        )));
    }

    // Reload configuration
    reload_config(&state).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Routing Rules Endpoints
// ============================================================================

#[derive(Debug, Serialize)]
struct RoutingRuleResponse {
    id: i64,
    prefix: String,
    provider: String,
    priority: i64,
    enabled: bool,
    description: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Deserialize)]
struct CreateRoutingRuleRequest {
    prefix: String,
    provider: String,
    #[serde(default = "default_priority")]
    priority: i64,
    #[serde(default = "default_true")]
    enabled: bool,
    description: Option<String>,
}

fn default_priority() -> i64 {
    100
}

#[derive(Debug, Deserialize)]
struct UpdateRoutingRuleRequest {
    provider: Option<String>,
    priority: Option<i64>,
    enabled: Option<bool>,
    description: Option<String>,
}

/// GET /api/config/routing/rules
async fn list_routing_rules(
    State(state): State<ConfigApiState>,
) -> Result<Json<Vec<RoutingRuleResponse>>, AppError> {
    #[derive(sqlx::FromRow)]
    struct RoutingRuleRow {
        id: i64,
        prefix: String,
        provider: String,
        priority: i64,
        enabled: i64,
        description: Option<String>,
        created_at: i64,
        updated_at: i64,
    }

    let rows = sqlx::query_as::<_, RoutingRuleRow>(
        r#"
        SELECT id, prefix, provider, priority, enabled, description, created_at, updated_at
        FROM routing_rules
        WHERE deleted_at IS NULL
        ORDER BY priority ASC, created_at ASC
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let rules = rows
        .into_iter()
        .map(|row| RoutingRuleResponse {
            id: row.id,
            prefix: row.prefix,
            provider: row.provider,
            priority: row.priority,
            enabled: row.enabled != 0,
            description: row.description,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect();

    Ok(Json(rules))
}

/// POST /api/config/routing/rules
async fn create_routing_rule(
    State(state): State<ConfigApiState>,
    Json(req): Json<CreateRoutingRuleRequest>,
) -> Result<Json<RoutingRuleResponse>, AppError> {
    // Validate prefix (alphanumeric + dash/underscore/dot, 1-64 chars)
    if req.prefix.is_empty() || req.prefix.len() > 64 {
        return Err(AppError::ConfigError(
            "Invalid prefix: must be 1-64 characters".to_string(),
        ));
    }

    // Validate provider (must be openai, anthropic, or gemini)
    if !["openai", "anthropic", "gemini"].contains(&req.provider.as_str()) {
        return Err(AppError::ConfigError(
            "Invalid provider: must be openai, anthropic, or gemini".to_string(),
        ));
    }

    let now = chrono::Utc::now().timestamp_millis();
    let result = sqlx::query(
        r#"
        INSERT INTO routing_rules (prefix, provider, priority, enabled, description, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&req.prefix)
    .bind(&req.provider)
    .bind(req.priority)
    .bind(if req.enabled { 1 } else { 0 })
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::ConfigError(format!("Routing rule for prefix '{}' already exists", req.prefix))
        } else {
            AppError::InternalError(format!("Database error: {}", e))
        }
    })?;

    let id = result.last_insert_rowid();

    // Reload configuration
    reload_config(&state).await?;

    Ok(Json(RoutingRuleResponse {
        id,
        prefix: req.prefix,
        provider: req.provider,
        priority: req.priority,
        enabled: req.enabled,
        description: req.description,
        created_at: now,
        updated_at: now,
    }))
}

/// PUT /api/config/routing/rules/:id
async fn update_routing_rule(
    State(state): State<ConfigApiState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateRoutingRuleRequest>,
) -> Result<Json<RoutingRuleResponse>, AppError> {
    // Validate provider if provided
    if let Some(provider) = &req.provider {
        if !["openai", "anthropic", "gemini"].contains(&provider.as_str()) {
            return Err(AppError::ConfigError(
                "Invalid provider: must be openai, anthropic, or gemini".to_string(),
            ));
        }
    }

    let now = chrono::Utc::now().timestamp_millis();

    // Build dynamic UPDATE query
    let mut updates = Vec::new();
    if req.provider.is_some() {
        updates.push("provider = ?");
    }
    if req.priority.is_some() {
        updates.push("priority = ?");
    }
    if req.enabled.is_some() {
        updates.push("enabled = ?");
    }
    if req.description.is_some() {
        updates.push("description = ?");
    }

    let mut query = String::from("UPDATE routing_rules SET updated_at = ?");
    if !updates.is_empty() {
        query.push_str(", ");
        query.push_str(&updates.join(", "));
    }
    query.push_str(" WHERE id = ? AND deleted_at IS NULL");

    let mut q = sqlx::query(&query).bind(now);
    if let Some(provider) = &req.provider {
        q = q.bind(provider);
    }
    if let Some(priority) = req.priority {
        q = q.bind(priority);
    }
    if let Some(enabled) = req.enabled {
        q = q.bind(if enabled { 1 } else { 0 });
    }
    if let Some(desc) = &req.description {
        q = q.bind(desc);
    }
    q = q.bind(id);

    let result = q.execute(&state.db_pool).await.map_err(|e| {
        AppError::InternalError(format!("Database error: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(AppError::ConfigError(format!(
            "Routing rule with id {} not found",
            id
        )));
    }

    // Reload configuration
    reload_config(&state).await?;

    // Fetch updated record
    #[derive(sqlx::FromRow)]
    struct RoutingRuleRow {
        id: i64,
        prefix: String,
        provider: String,
        priority: i64,
        enabled: i64,
        description: Option<String>,
        created_at: i64,
        updated_at: i64,
    }

    let row = sqlx::query_as::<_, RoutingRuleRow>(
        r#"
        SELECT id, prefix, provider, priority, enabled, description, created_at, updated_at
        FROM routing_rules
        WHERE id = ? AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    Ok(Json(RoutingRuleResponse {
        id: row.id,
        prefix: row.prefix,
        provider: row.provider,
        priority: row.priority,
        enabled: row.enabled != 0,
        description: row.description,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }))
}

/// DELETE /api/config/routing/rules/:id
async fn delete_routing_rule(
    State(state): State<ConfigApiState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let now = chrono::Utc::now().timestamp_millis();

    let result = sqlx::query(
        r#"
        UPDATE routing_rules
        SET deleted_at = ?
        WHERE id = ? AND deleted_at IS NULL
        "#,
    )
    .bind(now)
    .bind(id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::ConfigError(format!(
            "Routing rule with id {} not found",
            id
        )));
    }

    // Reload configuration
    reload_config(&state).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Routing Global Config Endpoints
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct RoutingConfigResponse {
    default_provider: Option<String>,
    discovery_enabled: bool,
    discovery_cache_ttl_seconds: i64,
    discovery_refresh_on_startup: bool,
    discovery_providers_with_listing: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateRoutingConfigRequest {
    default_provider: Option<String>,
    discovery_enabled: Option<bool>,
    discovery_cache_ttl_seconds: Option<i64>,
    discovery_refresh_on_startup: Option<bool>,
    discovery_providers_with_listing: Option<Vec<String>>,
}

/// GET /api/config/routing/global
async fn get_routing_config(
    State(state): State<ConfigApiState>,
) -> Result<Json<RoutingConfigResponse>, AppError> {
    #[derive(sqlx::FromRow)]
    struct RoutingConfigRow {
        default_provider: Option<String>,
        discovery_enabled: i64,
        discovery_cache_ttl_seconds: i64,
        discovery_refresh_on_startup: i64,
        discovery_providers_with_listing: String,
    }

    let row = sqlx::query_as::<_, RoutingConfigRow>(
        r#"
        SELECT
            default_provider,
            discovery_enabled,
            discovery_cache_ttl_seconds,
            discovery_refresh_on_startup,
            discovery_providers_with_listing
        FROM routing_config
        WHERE id = 1
        "#,
    )
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let providers_with_listing: Vec<String> =
        serde_json::from_str(&row.discovery_providers_with_listing)
            .unwrap_or_else(|_| vec!["openai".to_string()]);

    Ok(Json(RoutingConfigResponse {
        default_provider: row.default_provider,
        discovery_enabled: row.discovery_enabled != 0,
        discovery_cache_ttl_seconds: row.discovery_cache_ttl_seconds,
        discovery_refresh_on_startup: row.discovery_refresh_on_startup != 0,
        discovery_providers_with_listing: providers_with_listing,
    }))
}

/// PUT /api/config/routing/global
async fn update_routing_config(
    State(state): State<ConfigApiState>,
    Json(req): Json<UpdateRoutingConfigRequest>,
) -> Result<Json<RoutingConfigResponse>, AppError> {
    let now = chrono::Utc::now().timestamp_millis();

    // Build dynamic UPDATE query
    let mut updates = Vec::new();
    if req.default_provider.is_some() {
        updates.push("default_provider = ?");
    }
    if req.discovery_enabled.is_some() {
        updates.push("discovery_enabled = ?");
    }
    if req.discovery_cache_ttl_seconds.is_some() {
        updates.push("discovery_cache_ttl_seconds = ?");
    }
    if req.discovery_refresh_on_startup.is_some() {
        updates.push("discovery_refresh_on_startup = ?");
    }
    if req.discovery_providers_with_listing.is_some() {
        updates.push("discovery_providers_with_listing = ?");
    }

    let mut query = String::from("UPDATE routing_config SET updated_at = ?");
    if !updates.is_empty() {
        query.push_str(", ");
        query.push_str(&updates.join(", "));
    }
    query.push_str(" WHERE id = 1");

    let mut q = sqlx::query(&query).bind(now);
    if let Some(provider) = &req.default_provider {
        q = q.bind(provider);
    }
    if let Some(enabled) = req.discovery_enabled {
        q = q.bind(if enabled { 1 } else { 0 });
    }
    if let Some(ttl) = req.discovery_cache_ttl_seconds {
        q = q.bind(ttl);
    }
    if let Some(refresh) = req.discovery_refresh_on_startup {
        q = q.bind(if refresh { 1 } else { 0 });
    }
    if let Some(providers) = &req.discovery_providers_with_listing {
        let json = serde_json::to_string(providers).unwrap();
        q = q.bind(json);
    }

    q.execute(&state.db_pool)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    // Reload configuration
    reload_config(&state).await?;

    // Fetch updated config
    get_routing_config(State(state)).await
}

// ============================================================================
// Provider Instances Endpoints (Continued in next part due to length)
// ============================================================================

#[derive(Debug, Serialize)]
struct ProviderInstanceResponse {
    id: i64,
    provider: String,
    name: String,
    enabled: bool,
    base_url: String,
    timeout_seconds: i64,
    priority: i64,
    weight: i64,
    failure_timeout_seconds: i64,
    extra_config: Option<serde_json::Value>,
    description: Option<String>,
    created_at: i64,
    updated_at: i64,
    health_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateProviderInstanceRequest {
    name: String,
    api_key: String,
    base_url: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_timeout")]
    timeout_seconds: i64,
    #[serde(default = "default_priority_i64")]
    priority: i64,
    #[serde(default = "default_weight_i64")]
    weight: i64,
    #[serde(default = "default_failure_timeout_i64")]
    failure_timeout_seconds: i64,
    extra_config: Option<serde_json::Value>,
    description: Option<String>,
}

fn default_timeout() -> i64 {
    300
}

fn default_priority_i64() -> i64 {
    1
}

fn default_weight_i64() -> i64 {
    100
}

fn default_failure_timeout_i64() -> i64 {
    60
}

#[derive(Debug, Deserialize)]
struct UpdateProviderInstanceRequest {
    enabled: Option<bool>,
    api_key: Option<String>,
    base_url: Option<String>,
    timeout_seconds: Option<i64>,
    priority: Option<i64>,
    weight: Option<i64>,
    failure_timeout_seconds: Option<i64>,
    extra_config: Option<serde_json::Value>,
    description: Option<String>,
}

/// GET /api/config/providers/:provider/instances
async fn list_provider_instances(
    State(state): State<ConfigApiState>,
    Path(provider): Path<String>,
) -> Result<Json<Vec<ProviderInstanceResponse>>, AppError> {
    // Validate provider
    if !["openai", "anthropic", "gemini"].contains(&provider.as_str()) {
        return Err(AppError::ConfigError(
            "Invalid provider: must be openai, anthropic, or gemini".to_string(),
        ));
    }

    #[derive(sqlx::FromRow)]
    struct ProviderInstanceRow {
        id: i64,
        provider: String,
        name: String,
        enabled: i64,
        base_url: String,
        timeout_seconds: i64,
        priority: i64,
        weight: i64,
        failure_timeout_seconds: i64,
        extra_config: Option<String>,
        description: Option<String>,
        created_at: i64,
        updated_at: i64,
        health_status: Option<String>,
    }

    let rows = sqlx::query_as::<_, ProviderInstanceRow>(
        r#"
        SELECT
            id, provider, name, enabled, base_url, timeout_seconds,
            priority, weight, failure_timeout_seconds, extra_config,
            description, created_at, updated_at, health_status
        FROM provider_instances
        WHERE provider = ? AND deleted_at IS NULL
        ORDER BY priority ASC, created_at ASC
        "#,
    )
    .bind(&provider)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let instances = rows
        .into_iter()
        .map(|row| {
            let extra_config = row
                .extra_config
                .and_then(|s| serde_json::from_str(&s).ok());

            ProviderInstanceResponse {
                id: row.id,
                provider: row.provider,
                name: row.name,
                enabled: row.enabled != 0,
                base_url: row.base_url,
                timeout_seconds: row.timeout_seconds,
                priority: row.priority,
                weight: row.weight,
                failure_timeout_seconds: row.failure_timeout_seconds,
                extra_config,
                description: row.description,
                created_at: row.created_at,
                updated_at: row.updated_at,
                health_status: row.health_status,
            }
        })
        .collect();

    Ok(Json(instances))
}

/// POST /api/config/providers/:provider/instances
async fn create_provider_instance(
    State(state): State<ConfigApiState>,
    Path(provider): Path<String>,
    Json(req): Json<CreateProviderInstanceRequest>,
) -> Result<Json<ProviderInstanceResponse>, AppError> {
    // Validate provider
    if !["openai", "anthropic", "gemini"].contains(&provider.as_str()) {
        return Err(AppError::ConfigError(
            "Invalid provider: must be openai, anthropic, or gemini".to_string(),
        ));
    }

    // Validate name
    if !req
        .name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        || req.name.is_empty()
        || req.name.len() > 64
    {
        return Err(AppError::ConfigError(
            "Invalid name: must be alphanumeric with dash/underscore, 1-64 chars".to_string(),
        ));
    }

    // Store API key as-is (plaintext)
    // Note: The field is named api_key_encrypted but we store plaintext for now
    // because gateway needs to use the actual key to call upstream providers.
    // TODO: Implement proper encryption/decryption if needed
    let api_key_plaintext = req.api_key.clone();

    // Serialize extra_config to JSON string
    let extra_config_json = req
        .extra_config
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap());

    let now = chrono::Utc::now().timestamp_millis();
    let result = sqlx::query(
        r#"
        INSERT INTO provider_instances
        (provider, name, enabled, api_key_encrypted, base_url, timeout_seconds,
         priority, weight, failure_timeout_seconds, extra_config, description,
         created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&provider)
    .bind(&req.name)
    .bind(if req.enabled { 1 } else { 0 })
    .bind(&api_key_plaintext)
    .bind(&req.base_url)
    .bind(req.timeout_seconds)
    .bind(req.priority)
    .bind(req.weight)
    .bind(req.failure_timeout_seconds)
    .bind(&extra_config_json)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::ConfigError(format!(
                "Provider instance '{}' already exists for {}",
                req.name, provider
            ))
        } else {
            AppError::InternalError(format!("Database error: {}", e))
        }
    })?;

    let id = result.last_insert_rowid();

    // Reload configuration and load balancers
    reload_config_and_load_balancers(&state).await?;

    Ok(Json(ProviderInstanceResponse {
        id,
        provider: provider.clone(),
        name: req.name,
        enabled: req.enabled,
        base_url: req.base_url,
        timeout_seconds: req.timeout_seconds,
        priority: req.priority,
        weight: req.weight,
        failure_timeout_seconds: req.failure_timeout_seconds,
        extra_config: req.extra_config,
        description: req.description,
        created_at: now,
        updated_at: now,
        health_status: Some("unknown".to_string()),
    }))
}

/// PUT /api/config/providers/:provider/instances/:name
async fn update_provider_instance(
    State(state): State<ConfigApiState>,
    Path((provider, name)): Path<(String, String)>,
    Json(req): Json<UpdateProviderInstanceRequest>,
) -> Result<Json<ProviderInstanceResponse>, AppError> {
    // Validate provider
    if !["openai", "anthropic", "gemini"].contains(&provider.as_str()) {
        return Err(AppError::ConfigError(
            "Invalid provider: must be openai, anthropic, or gemini".to_string(),
        ));
    }

    let now = chrono::Utc::now().timestamp_millis();

    // Build dynamic UPDATE query
    let mut updates = Vec::new();
    if req.enabled.is_some() {
        updates.push("enabled = ?");
    }
    if req.api_key.is_some() {
        updates.push("api_key_encrypted = ?");
    }
    if req.base_url.is_some() {
        updates.push("base_url = ?");
    }
    if req.timeout_seconds.is_some() {
        updates.push("timeout_seconds = ?");
    }
    if req.priority.is_some() {
        updates.push("priority = ?");
    }
    if req.weight.is_some() {
        updates.push("weight = ?");
    }
    if req.failure_timeout_seconds.is_some() {
        updates.push("failure_timeout_seconds = ?");
    }
    if req.extra_config.is_some() {
        updates.push("extra_config = ?");
    }
    if req.description.is_some() {
        updates.push("description = ?");
    }

    let mut query = String::from("UPDATE provider_instances SET updated_at = ?");
    if !updates.is_empty() {
        query.push_str(", ");
        query.push_str(&updates.join(", "));
    }
    query.push_str(" WHERE provider = ? AND name = ? AND deleted_at IS NULL");

    let mut q = sqlx::query(&query).bind(now);
    if let Some(enabled) = req.enabled {
        q = q.bind(if enabled { 1 } else { 0 });
    }
    if let Some(api_key) = &req.api_key {
        // Store API key as-is (plaintext) - same reason as in create
        q = q.bind(api_key);
    }
    if let Some(base_url) = &req.base_url {
        q = q.bind(base_url);
    }
    if let Some(timeout) = req.timeout_seconds {
        q = q.bind(timeout);
    }
    if let Some(priority) = req.priority {
        q = q.bind(priority);
    }
    if let Some(weight) = req.weight {
        q = q.bind(weight);
    }
    if let Some(failure_timeout) = req.failure_timeout_seconds {
        q = q.bind(failure_timeout);
    }
    if let Some(extra_config) = &req.extra_config {
        let json = serde_json::to_string(extra_config).unwrap();
        q = q.bind(json);
    }
    if let Some(desc) = &req.description {
        q = q.bind(desc);
    }
    q = q.bind(&provider).bind(&name);

    let result = q.execute(&state.db_pool).await.map_err(|e| {
        AppError::InternalError(format!("Database error: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(AppError::ConfigError(format!(
            "Provider instance '{}' not found for {}",
            name, provider
        )));
    }

    // Reload configuration and load balancers
    reload_config_and_load_balancers(&state).await?;

    // Fetch updated record
    #[derive(sqlx::FromRow)]
    struct ProviderInstanceRow {
        id: i64,
        provider: String,
        name: String,
        enabled: i64,
        base_url: String,
        timeout_seconds: i64,
        priority: i64,
        weight: i64,
        failure_timeout_seconds: i64,
        extra_config: Option<String>,
        description: Option<String>,
        created_at: i64,
        updated_at: i64,
        health_status: Option<String>,
    }

    let row = sqlx::query_as::<_, ProviderInstanceRow>(
        r#"
        SELECT
            id, provider, name, enabled, base_url, timeout_seconds,
            priority, weight, failure_timeout_seconds, extra_config,
            description, created_at, updated_at, health_status
        FROM provider_instances
        WHERE provider = ? AND name = ? AND deleted_at IS NULL
        "#,
    )
    .bind(&provider)
    .bind(&name)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let extra_config = row
        .extra_config
        .and_then(|s| serde_json::from_str(&s).ok());

    Ok(Json(ProviderInstanceResponse {
        id: row.id,
        provider: row.provider,
        name: row.name,
        enabled: row.enabled != 0,
        base_url: row.base_url,
        timeout_seconds: row.timeout_seconds,
        priority: row.priority,
        weight: row.weight,
        failure_timeout_seconds: row.failure_timeout_seconds,
        extra_config,
        description: row.description,
        created_at: row.created_at,
        updated_at: row.updated_at,
        health_status: row.health_status,
    }))
}

/// DELETE /api/config/providers/:provider/instances/:name
async fn delete_provider_instance(
    State(state): State<ConfigApiState>,
    Path((provider, name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let now = chrono::Utc::now().timestamp_millis();

    let result = sqlx::query(
        r#"
        UPDATE provider_instances
        SET deleted_at = ?
        WHERE provider = ? AND name = ? AND deleted_at IS NULL
        "#,
    )
    .bind(now)
    .bind(&provider)
    .bind(&name)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::ConfigError(format!(
            "Provider instance '{}' not found for {}",
            name, provider
        )));
    }

    // Reload configuration and load balancers
    reload_config_and_load_balancers(&state).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Configuration Reload
// ============================================================================

/// Reload configuration from database (without rebuilding load balancers)
async fn reload_config(state: &ConfigApiState) -> Result<(), AppError> {
    let new_config = crate::config_db::load_config(&state.db_pool)
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to reload config: {}", e)))?;

    state.config.store(Arc::new(new_config));
    tracing::info!("Configuration reloaded from database");

    Ok(())
}

/// Reload configuration and rebuild load balancers
async fn reload_config_and_load_balancers(state: &ConfigApiState) -> Result<(), AppError> {
    // Reload config
    let new_config = crate::config_db::load_config(&state.db_pool)
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to reload config: {}", e)))?;

    // Rebuild load balancers
    let new_lb = build_load_balancers(&new_config, None);

    // Atomic update
    state.config.store(Arc::new(new_config));
    state.load_balancers.store(Arc::new((*new_lb).clone()));

    tracing::info!("Configuration and load balancers reloaded from database");

    Ok(())
}
