use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

/// PKCE parameters for OAuth flow
#[derive(Debug, Clone)]
pub struct PkceParams {
    pub code_verifier: String,
    pub code_challenge: String,
    pub state: String,
}

/// Generate PKCE parameters for OAuth authorization
pub fn generate_pkce_params() -> PkceParams {
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();

    PkceParams {
        code_verifier,
        code_challenge,
        state,
    }
}

/// Generate a random code verifier (43-128 characters)
fn generate_code_verifier() -> String {
    let mut rng = rand::thread_rng();
    let random_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    URL_SAFE_NO_PAD.encode(&random_bytes)
}

/// Generate code challenge from verifier using SHA256
fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate a random state parameter
fn generate_state() -> String {
    let mut rng = rand::thread_rng();
    let random_bytes: Vec<u8> = (0..16).map(|_| rng.gen()).collect();
    URL_SAFE_NO_PAD.encode(&random_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_pkce_params() {
        let params = generate_pkce_params();

        // Verify all fields are non-empty
        assert!(!params.code_verifier.is_empty());
        assert!(!params.code_challenge.is_empty());
        assert!(!params.state.is_empty());

        // Verify code_verifier length (base64 encoded 32 bytes ≈ 43 chars)
        assert!(params.code_verifier.len() >= 40);

        // Verify code_challenge is different from verifier
        assert_ne!(params.code_verifier, params.code_challenge);

        // Verify code_challenge is base64url encoded (SHA256 hash)
        assert!(params.code_challenge.len() >= 40);
    }

    #[test]
    fn test_pkce_params_are_unique() {
        let params1 = generate_pkce_params();
        let params2 = generate_pkce_params();

        // Each generation should produce unique values
        assert_ne!(params1.code_verifier, params2.code_verifier);
        assert_ne!(params1.code_challenge, params2.code_challenge);
        assert_ne!(params1.state, params2.state);
    }

    #[test]
    fn test_code_challenge_deterministic() {
        let verifier = "test_verifier_12345678901234567890";
        let challenge1 = generate_code_challenge(verifier);
        let challenge2 = generate_code_challenge(verifier);

        // Same verifier should produce same challenge
        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn test_code_challenge_is_sha256() {
        let verifier = "test_verifier";
        let challenge = generate_code_challenge(verifier);

        // Verify it's base64url encoded (no padding, URL-safe)
        assert!(!challenge.contains('='));
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));

        // SHA256 hash is 32 bytes, base64 encoded is 43 chars
        assert_eq!(challenge.len(), 43);
    }
}
