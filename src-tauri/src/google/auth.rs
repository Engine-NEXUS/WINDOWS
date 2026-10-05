use super::types::GoogleError;

pub const GOOGLE_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/photoslibrary.readonly",
];

pub struct GoogleAuth;

impl GoogleAuth {
    /// Retrieve a valid OAuth2 access token for Google API calls.
    pub fn get_access_token() -> Result<String, GoogleError> {
        // 1. Check in-memory / OS credential vault
        if let Some(token) = crate::auth_vault::get_token("google") {
            if !token.trim().is_empty() {
                return Ok(token);
            }
        }

        // 2. Check environment override (useful in CLI / CI / tests)
        if let Ok(env_token) = std::env::var("GOOGLE_ACCESS_TOKEN") {
            if !env_token.trim().is_empty() {
                return Ok(env_token);
            }
        }

        Err(GoogleError::AuthRequired)
    }

    /// Check if Google account is currently authenticated with a valid token.
    pub fn is_authenticated() -> bool {
        Self::get_access_token().is_ok()
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_unauthenticated_without_env() {
        // Clear vault token for test if any
        crate::auth_vault::clear_token("google");
        std::env::remove_var("GOOGLE_ACCESS_TOKEN");

        let res = GoogleAuth::get_access_token();
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), GoogleError::AuthRequired);
    }

    #[test]
    fn test_auth_with_env_token() {
        std::env::set_var("GOOGLE_ACCESS_TOKEN", "mock_ya29_token_123");
        let token = GoogleAuth::get_access_token().unwrap();
        assert_eq!(token, "mock_ya29_token_123");
        assert!(GoogleAuth::is_authenticated());
        std::env::remove_var("GOOGLE_ACCESS_TOKEN");
    }
}
