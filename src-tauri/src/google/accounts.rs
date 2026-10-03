//! Google Accounts Sub-Engine for Multi-Account Registry & OAuth Management.

use super::oauth;
use super::types::GoogleAccountProfile;

/// Retrieve all connected Google account profiles.
pub fn list_accounts() -> Vec<GoogleAccountProfile> {
    crate::auth_vault::get_google_accounts()
}

/// Start RFC 8252 direct native loopback OAuth flow to connect a new Google account.
pub async fn connect_account() -> Result<GoogleAccountProfile, String> {
    oauth::start_loopback_oauth_flow().await
}

/// Disconnect and remove a connected Google account.
pub fn disconnect_account(email: &str) -> Result<(), String> {
    crate::auth_vault::remove_google_account(email)
}

/// Set the primary Google account by email.
pub fn set_primary_account(email: &str) -> Result<(), String> {
    crate::auth_vault::set_primary_google_account(email)
}

/// Save custom Developer Client ID and Client Secret in vault.
pub fn save_custom_credentials(client_id: &str, client_secret: Option<&str>) {
    crate::auth_vault::set_api_key("google_client_id", client_id);
    if let Some(sec) = client_secret {
        crate::auth_vault::set_api_key("google_client_secret", sec);
    } else {
        crate::auth_vault::clear_api_key("google_client_secret");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_credentials_storage() {
        save_custom_credentials("test_client_id_123", Some("test_secret_456"));
        let (id, sec) = oauth::get_client_credentials();
        assert_eq!(id, "test_client_id_123");
        assert_eq!(sec.as_deref(), Some("test_secret_456"));

        // Cleanup
        crate::auth_vault::clear_api_key("google_client_id");
        crate::auth_vault::clear_api_key("google_client_secret");
    }
}
