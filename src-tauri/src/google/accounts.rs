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

/// Progressive re-consent for phone + address (sensitive scopes).
/// Merges into the existing profile — never a duplicate account.
pub async fn connect_extended() -> Result<GoogleAccountProfile, String> {
    oauth::connect_extended_profile().await
}

/// Normalise an email address by trimming surrounding whitespace and converting to lowercase. Pure.
pub fn normalise_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Disconnect and remove a connected Google account and its associated tokens.
pub fn disconnect_account(email: &str) -> Result<(), String> {
    let normalised = normalise_email(email);
    crate::auth_vault::remove_google_account(&normalised)?;
    // If all accounts are now disconnected, clear legacy single-account token
    if list_accounts().is_empty() {
        crate::auth_vault::clear_token("google");
    }
    Ok(())
}

/// Set the primary Google account by email.
pub fn set_primary_account(email: &str) -> Result<(), String> {
    let normalised = normalise_email(email);
    crate::auth_vault::set_primary_google_account(&normalised)
}

/// Save custom Developer Client ID and Client Secret in vault. Rejects a
/// malformed id up front (nothing is stored), so a typo is caught here and
/// not on Google's error page.
pub fn save_custom_credentials(client_id: &str, client_secret: Option<&str>) -> Result<(), String> {
    let id = client_id.trim();
    if !oauth::is_valid_client_id(id) {
        return Err("That Client ID does not look right. It should look like 123456789012-abcdef….apps.googleusercontent.com (copy it from Google Cloud Console → Credentials).".to_string());
    }
    let secret = client_secret.map(str::trim).filter(|s| !s.is_empty());
    crate::auth_vault::set_api_key("google_client_id", id);
    match secret {
        Some(sec) => crate::auth_vault::set_api_key("google_client_secret", sec),
        None => crate::auth_vault::clear_api_key("google_client_secret"),
    }
    Ok(())
}

/// Whether sign-in can start (never reveals the secret).
pub fn credentials_status() -> oauth::CredentialsStatus {
    oauth::credentials_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_credentials_storage() {
        // A malformed id is refused and stores nothing.
        assert!(save_custom_credentials("test_client_id_123", Some("x")).is_err());
        let good = "123456789012-abcdefghijklmnop1234567890abcdef.apps.googleusercontent.com";
        save_custom_credentials(good, Some("  test_secret_456 ")).unwrap();
        let (id, sec) = oauth::get_client_credentials();
        assert_eq!(id, good);
        assert_eq!(sec.as_deref(), Some("test_secret_456"));
        let st = credentials_status();
        assert!(st.configured && st.source == "saved" && st.message.is_none());
        // Saving without a secret leaves sign-in not ready, with a precise reason.
        save_custom_credentials(good, None).unwrap();
        let st = credentials_status();
        assert!(!st.configured && st.has_client_id && !st.has_secret);

        // Cleanup
        crate::auth_vault::clear_api_key("google_client_id");
        crate::auth_vault::clear_api_key("google_client_secret");
    }

    fn make_test_profile(email: &str, name: &str, is_primary: bool) -> GoogleAccountProfile {
        GoogleAccountProfile {
            email: email.to_string(),
            name: name.to_string(),
            picture: None,
            is_primary,
            added_at_ms: 1000,
            scopes: vec!["gmail.readonly".into()],
            phone: None,
            address: None,
        }
    }

    fn clean_test_accounts() {
        for a in list_accounts() {
            let _ = disconnect_account(&a.email);
        }
    }

    #[test]
    fn email_normalisation_trims_and_lowercases() {
        assert_eq!(normalise_email("  User.Name+Tag@GMAIL.COM \t\r\n"), "user.name+tag@gmail.com");
        assert_eq!(normalise_email("Test@Example.Com"), "test@example.com");
        assert_eq!(normalise_email("already.clean@gmail.com"), "already.clean@gmail.com");
        assert_eq!(normalise_email(""), "");
    }

    #[test]
    fn accounts_first_registered_becomes_primary() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        let prof = make_test_profile("first@example.com", "First User", false);
        crate::auth_vault::save_google_account(prof, Some("rt_first"));

        let accounts = list_accounts();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "first@example.com");
        assert!(accounts[0].is_primary, "first account must automatically become primary");

        clean_test_accounts();
    }

    #[test]
    fn accounts_subsequent_registered_is_not_primary_by_default() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::save_google_account(make_test_profile("first@example.com", "First", true), Some("rt_first"));
        crate::auth_vault::save_google_account(make_test_profile("second@example.com", "Second", false), Some("rt_second"));

        let accounts = list_accounts();
        assert_eq!(accounts.len(), 2);
        let first = accounts.iter().find(|a| a.email == "first@example.com").unwrap();
        let second = accounts.iter().find(|a| a.email == "second@example.com").unwrap();
        assert!(first.is_primary);
        assert!(!second.is_primary);

        clean_test_accounts();
    }

    #[test]
    fn accounts_switch_primary_promotes_target_and_demotes_others() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::save_google_account(make_test_profile("one@example.com", "One", true), Some("rt_one"));
        crate::auth_vault::save_google_account(make_test_profile("two@example.com", "Two", false), Some("rt_two"));

        set_primary_account("two@example.com").unwrap();

        let accounts = list_accounts();
        let one = accounts.iter().find(|a| a.email == "one@example.com").unwrap();
        let two = accounts.iter().find(|a| a.email == "two@example.com").unwrap();
        assert!(!one.is_primary, "previous primary must be demoted");
        assert!(two.is_primary, "new account must be promoted to primary");

        clean_test_accounts();
    }

    #[test]
    fn accounts_set_primary_case_insensitive() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::save_google_account(make_test_profile("user@example.com", "User", false), Some("rt_u"));

        set_primary_account("  USER@EXAMPLE.COM  ").unwrap();
        let accounts = list_accounts();
        assert!(accounts[0].is_primary);

        clean_test_accounts();
    }

    #[test]
    fn accounts_duplicate_email_updates_existing_entry() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::save_google_account(make_test_profile("user@example.com", "Old Name", false), Some("rt_1"));
        assert_eq!(list_accounts().len(), 1);

        // Save with different case and new name
        crate::auth_vault::save_google_account(make_test_profile("USER@EXAMPLE.COM", "New Name", false), None);

        let accounts = list_accounts();
        assert_eq!(accounts.len(), 1, "duplicate email must update existing entry rather than duplicating");
        assert_eq!(accounts[0].name, "New Name");

        clean_test_accounts();
    }

    #[test]
    fn accounts_remove_primary_promotes_next_remaining() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::save_google_account(make_test_profile("primary@example.com", "P", true), Some("rt_p"));
        crate::auth_vault::save_google_account(make_test_profile("secondary@example.com", "S", false), Some("rt_s"));

        disconnect_account("primary@example.com").unwrap();

        let accounts = list_accounts();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "secondary@example.com");
        assert!(accounts[0].is_primary, "remaining account must be promoted to primary upon primary removal");

        clean_test_accounts();
    }

    #[test]
    fn accounts_remove_last_account_leaves_registry_empty() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::save_google_account(make_test_profile("sole@example.com", "Sole", true), Some("rt_sole"));
        assert_eq!(list_accounts().len(), 1);

        disconnect_account("sole@example.com").unwrap();
        assert!(list_accounts().is_empty(), "removing last account must leave registry empty");

        clean_test_accounts();
    }

    #[test]
    fn accounts_disconnect_clears_tokens_from_keychain_mock() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::save_google_account(make_test_profile("token_test@example.com", "T", true), Some("refresh_secret_123"));
        crate::auth_vault::set_google_access_token("token_test@example.com", "access_secret_456", 3600.0);

        assert_eq!(crate::auth_vault::get_google_refresh_token("token_test@example.com").as_deref(), Some("refresh_secret_123"));
        assert_eq!(crate::auth_vault::get_google_access_token("token_test@example.com").as_deref(), Some("access_secret_456"));

        disconnect_account("token_test@example.com").unwrap();

        assert!(crate::auth_vault::get_google_refresh_token("token_test@example.com").is_none(), "refresh token must be cleared on disconnect");
        assert!(crate::auth_vault::get_google_access_token("token_test@example.com").is_none(), "access token must be cleared on disconnect");

        clean_test_accounts();
    }

    #[test]
    fn accounts_disconnect_last_account_clears_legacy_google_token() {
        let _g = crate::auth_vault::lock_accounts_for_test();
        clean_test_accounts();
        crate::auth_vault::set_token("google", "legacy_token_123", 3600.0);
        crate::auth_vault::save_google_account(make_test_profile("legacy@example.com", "L", true), Some("rt_leg"));

        assert_eq!(crate::auth_vault::get_token("google").as_deref(), Some("legacy_token_123"));

        disconnect_account("legacy@example.com").unwrap();
        assert!(list_accounts().is_empty());
        assert!(crate::auth_vault::get_token("google").is_none(), "legacy google token must be cleared when all accounts are removed");

        clean_test_accounts();
    }
}
