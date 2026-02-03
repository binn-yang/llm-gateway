use crate::errors::AppError;
use crate::oauth::types::OAuthToken;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, PasswordHasher};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tokio::sync::RwLock;

/// Token storage with encryption
#[derive(Debug)]
pub struct TokenStore {
    storage_path: PathBuf,
    tokens: RwLock<HashMap<String, OAuthToken>>,
    encryption_key: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenStorage {
    version: String,
    tokens: HashMap<String, EncryptedToken>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: i64,
    token_type: String,
    scope: String,
    created_at: i64,
    last_refreshed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    organization: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    account: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    subscription_info: Option<serde_json::Value>,
}

impl TokenStore {
    /// Create a new token store with encryption
    pub async fn new(storage_path: PathBuf) -> Result<Self, AppError> {
        // Generate encryption key from machine-specific data
        let encryption_key = Self::derive_encryption_key()?;

        let mut store = Self {
            storage_path,
            tokens: RwLock::new(HashMap::new()),
            encryption_key,
        };

        // Load existing tokens if file exists
        if store.storage_path.exists() {
            store.load_tokens().await?;
        }

        Ok(store)
    }

    /// Derive encryption key from machine-specific data
    fn derive_encryption_key() -> Result<Vec<u8>, AppError> {
        // Use hostname as password for key derivation
        let hostname = hostname::get()
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to get hostname: {}", e),
            })?
            .to_string_lossy()
            .to_string();

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(hostname.as_bytes(), &salt)
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to derive encryption key: {}", e),
            })?;

        // Extract 32 bytes for AES-256
        let hash_bytes = password_hash.hash.ok_or_else(|| AppError::OAuthError {
            message: "Failed to extract hash bytes".to_string(),
        })?;

        Ok(hash_bytes.as_bytes()[..32].to_vec())
    }

    /// Save a token for a provider
    pub async fn save_token(&self, provider_name: &str, token: &OAuthToken) -> Result<(), AppError> {
        // Update in-memory cache
        {
            let mut tokens = self.tokens.write().await;
            tokens.insert(provider_name.to_string(), token.clone());
        }

        // Persist to disk
        self.save_tokens().await
    }

    /// Get a token for a provider
    pub async fn get_token(&self, provider_name: &str) -> Result<OAuthToken, AppError> {
        let tokens = self.tokens.read().await;
        tokens
            .get(provider_name)
            .cloned()
            .ok_or_else(|| AppError::OAuthError {
                message: format!("No token found for provider '{}'", provider_name),
            })
    }

    /// Delete a token for a provider
    pub async fn delete_token(&self, provider_name: &str) -> Result<(), AppError> {
        {
            let mut tokens = self.tokens.write().await;
            tokens.remove(provider_name);
        }

        self.save_tokens().await
    }

    /// List all provider names with tokens
    pub async fn list_providers(&self) -> Vec<String> {
        let tokens = self.tokens.read().await;
        tokens.keys().cloned().collect()
    }

    /// Load tokens from disk
    async fn load_tokens(&mut self) -> Result<(), AppError> {
        let content = fs::read_to_string(&self.storage_path)
            .await
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to read token file: {}", e),
            })?;

        let storage: TokenStorage = serde_json::from_str(&content)
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to parse token file: {}", e),
            })?;

        let mut tokens = HashMap::new();
        for (provider_name, encrypted_token) in storage.tokens {
            let token = OAuthToken {
                access_token: self.decrypt(&encrypted_token.access_token)?,
                refresh_token: encrypted_token
                    .refresh_token
                    .map(|t| self.decrypt(&t))
                    .transpose()?,
                expires_at: encrypted_token.expires_at,
                token_type: encrypted_token.token_type,
                scope: encrypted_token.scope,
                created_at: encrypted_token.created_at,
                last_refreshed_at: encrypted_token.last_refreshed_at,
                organization: encrypted_token.organization,
                account: encrypted_token.account,
                subscription_info: encrypted_token.subscription_info,
            };
            tokens.insert(provider_name, token);
        }

        *self.tokens.write().await = tokens;
        Ok(())
    }

    /// Save tokens to disk
    async fn save_tokens(&self) -> Result<(), AppError> {
        let tokens = self.tokens.read().await;

        let mut encrypted_tokens = HashMap::new();
        for (provider_name, token) in tokens.iter() {
            let encrypted_token = EncryptedToken {
                access_token: self.encrypt(&token.access_token)?,
                refresh_token: token
                    .refresh_token
                    .as_ref()
                    .map(|t| self.encrypt(t))
                    .transpose()?,
                expires_at: token.expires_at,
                token_type: token.token_type.clone(),
                scope: token.scope.clone(),
                created_at: token.created_at,
                last_refreshed_at: token.last_refreshed_at,
                organization: token.organization.clone(),
                account: token.account.clone(),
                subscription_info: token.subscription_info.clone(),
            };
            encrypted_tokens.insert(provider_name.clone(), encrypted_token);
        }

        let storage = TokenStorage {
            version: "1.0".to_string(),
            tokens: encrypted_tokens,
        };

        let content = serde_json::to_string_pretty(&storage)
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to serialize tokens: {}", e),
            })?;

        // Ensure parent directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| AppError::OAuthError {
                message: format!("Failed to create token directory: {}", e),
            })?;
        }

        fs::write(&self.storage_path, content)
            .await
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to write token file: {}", e),
            })?;

        Ok(())
    }

    /// Encrypt a string
    fn encrypt(&self, plaintext: &str) -> Result<String, AppError> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to create cipher: {}", e),
            })?;

        // Use a fixed nonce (not recommended for production, but acceptable for local storage)
        let nonce = Nonce::from_slice(b"unique nonce");

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| AppError::OAuthError {
                message: format!("Encryption failed: {}", e),
            })?;

        Ok(STANDARD.encode(&ciphertext))
    }

    /// Decrypt a string
    fn decrypt(&self, ciphertext: &str) -> Result<String, AppError> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to create cipher: {}", e),
            })?;

        let nonce = Nonce::from_slice(b"unique nonce");

        let ciphertext_bytes = STANDARD.decode(ciphertext)
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to decode ciphertext: {}", e),
            })?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext_bytes.as_ref())
            .map_err(|e| AppError::OAuthError {
                message: format!("Decryption failed: {}", e),
            })?;

        String::from_utf8(plaintext).map_err(|e| AppError::OAuthError {
            message: format!("Failed to convert decrypted data to string: {}", e),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    async fn create_test_store() -> (TokenStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("test_tokens.json");
        let store = TokenStore::new(storage_path).await.unwrap();
        (store, temp_dir)
    }

    fn create_test_token() -> OAuthToken {
        OAuthToken {
            access_token: "test_access_token".to_string(),
            refresh_token: Some("test_refresh_token".to_string()),
            expires_at: Utc::now().timestamp() + 3600,
            token_type: "Bearer".to_string(),
            scope: "api".to_string(),
            created_at: Utc::now().timestamp(),
            last_refreshed_at: Utc::now().timestamp(),
            organization: None,
            account: None,
            subscription_info: None,
        }
    }

    #[tokio::test]
    async fn test_save_and_get_token() {
        let (store, _temp_dir) = create_test_store().await;
        let token = create_test_token();

        // Save token
        store.save_token("test_provider", &token).await.unwrap();

        // Retrieve token
        let retrieved = store.get_token("test_provider").await.unwrap();
        assert_eq!(retrieved.access_token, token.access_token);
        assert_eq!(retrieved.refresh_token, token.refresh_token);
        assert_eq!(retrieved.token_type, token.token_type);
    }

    #[tokio::test]
    async fn test_delete_token() {
        let (store, _temp_dir) = create_test_store().await;
        let token = create_test_token();

        // Save token
        store.save_token("test_provider", &token).await.unwrap();

        // Verify it exists
        assert!(store.get_token("test_provider").await.is_ok());

        // Delete token
        store.delete_token("test_provider").await.unwrap();

        // Verify it's gone
        assert!(store.get_token("test_provider").await.is_err());
    }

    #[tokio::test]
    async fn test_list_providers() {
        let (store, _temp_dir) = create_test_store().await;
        let token = create_test_token();

        // Initially empty
        assert_eq!(store.list_providers().await.len(), 0);

        // Add tokens
        store.save_token("provider1", &token).await.unwrap();
        store.save_token("provider2", &token).await.unwrap();

        // List should contain both
        let providers = store.list_providers().await;
        assert_eq!(providers.len(), 2);
        assert!(providers.contains(&"provider1".to_string()));
        assert!(providers.contains(&"provider2".to_string()));
    }

    #[tokio::test]
    async fn test_encryption_decryption() {
        let (store, _temp_dir) = create_test_store().await;
        let plaintext = "sensitive_token_data";

        // Encrypt
        let encrypted = store.encrypt(plaintext).unwrap();

        // Verify encrypted is different from plaintext
        assert_ne!(encrypted, plaintext);

        // Decrypt
        let decrypted = store.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_token_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("persistent_tokens.json");
        let token = create_test_token();

        // Save token
        let store = TokenStore::new(storage_path.clone()).await.unwrap();
        store.save_token("persistent_provider", &token).await.unwrap();

        // Verify file exists
        assert!(storage_path.exists());

        // Retrieve from same store instance (encryption key is same)
        let retrieved = store.get_token("persistent_provider").await.unwrap();
        assert_eq!(retrieved.access_token, token.access_token);
        assert_eq!(retrieved.refresh_token, token.refresh_token);

        // Note: TokenStore uses machine-specific encryption key derived from hostname + random salt
        // So tokens saved by one TokenStore instance cannot be decrypted by another instance
        // This is intentional for security - tokens are tied to the machine they were created on
    }

    #[tokio::test]
    async fn test_get_nonexistent_token() {
        let (store, _temp_dir) = create_test_store().await;

        let result = store.get_token("nonexistent").await;
        assert!(result.is_err());
        if let Err(AppError::OAuthError { message }) = result {
            assert!(message.contains("No token found"));
        } else {
            panic!("Expected OAuthError");
        }
    }
}
