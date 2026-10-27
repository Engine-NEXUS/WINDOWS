//! Self-name and identity recognition for spoken notifications (spec C-P0).
//!
//! Keeps spoken notification wording free of the account holder's own name
//! and GitHub actor identity. Pure logic in `from_parts`, thin local-only
//! loader in `load`. Zero network, never panics.

use std::collections::HashSet;
use std::path::Path;

/// Lowercase tokens and full lowercase names representing the owner / self.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelfNames {
    pub full_names: HashSet<String>,
    pub tokens: HashSet<String>,
}

impl SelfNames {
    /// An empty set of names. Matches nothing.
    pub fn none() -> Self {
        Self {
            full_names: HashSet::new(),
            tokens: HashSet::new(),
        }
    }

    /// Whether this set contains no names or tokens.
    pub fn is_empty(&self) -> bool {
        self.full_names.is_empty() && self.tokens.is_empty()
    }

    /// True if the lowercased trimmed label equals a stored full name,
    /// OR every token of the label (length >= 2) is in the stored set,
    /// and the label has at least one token. An empty set is never "self".
    pub fn is_self(&self, label: &str) -> bool {
        if self.is_empty() {
            return false;
        }
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return false;
        }
        let lower = trimmed.to_lowercase();
        if self.full_names.contains(&lower) {
            return true;
        }
        let tokens: Vec<&str> = lower
            .split(|c: char| c.is_whitespace() || matches!(c, '.' | '_' | '-' | '@' | ',' | ':' | ';' | '/' | '(' | ')' | '[' | ']' | '<' | '>'))
            .map(|s| s.trim())
            .filter(|s| s.len() >= 2)
            .collect();

        if tokens.is_empty() {
            return false;
        }

        tokens.iter().all(|t| self.tokens.contains(*t))
    }

    /// Pure constructor from a slice of name/email strings.
    pub fn from_parts(parts: &[String]) -> Self {
        let mut full_names = HashSet::new();
        let mut tokens = HashSet::new();

        for part in parts {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_lowercase();
            full_names.insert(lower.clone());

            // For an email also add its local part
            if let Some((local, _)) = lower.split_once('@') {
                let local = local.trim();
                if !local.is_empty() {
                    tokens.insert(local.to_string());
                }
            }

            // Each whitespace/`.`/`_`/`-`/`@`-split token of length >= 3
            for tok in lower.split(|c: char| c.is_whitespace() || matches!(c, '.' | '_' | '-' | '@')) {
                let tok = tok.trim();
                if tok.len() >= 3 {
                    tokens.insert(tok.to_string());
                }
            }
        }

        Self { full_names, tokens }
    }

    /// Thin glue: reads local files only, never network, never panics.
    pub fn load(dir: &Path) -> Self {
        let mut parts = Vec::new();

        // 1. User profile name from memory
        if let Some(profile) = crate::memory::read_user_profile(dir) {
            if let Some(name) = profile.name {
                if !name.trim().is_empty() {
                    parts.push(name);
                }
            }
        }

        // 2. GitHub profile cache (login and name)
        let gh_path = dir.join("github_profile.json");
        if let Ok(content) = std::fs::read_to_string(&gh_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(login) = v.get("login").and_then(|x| x.as_str()) {
                    let login = login.trim();
                    if !login.is_empty() {
                        parts.push(login.to_string());
                    }
                }
                if let Some(name) = v.get("name").and_then(|x| x.as_str()) {
                    let name = name.trim();
                    if !name.is_empty() {
                        parts.push(name.to_string());
                    }
                }
            }
        }

        // 3. Every connected Google account (name and email)
        for account in crate::auth_vault::get_google_accounts() {
            if !account.name.trim().is_empty() {
                parts.push(account.name);
            }
            if !account.email.trim().is_empty() {
                parts.push(account.email);
            }
        }

        Self::from_parts(&parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_names_from_parts_and_matching() {
        let parts = vec![
            "Chitkul Lakshya".to_string(),
            "chitkullakshya@gmail.com".to_string(),
        ];
        let sn = SelfNames::from_parts(&parts);

        // Tokens split & full-name match
        assert!(sn.is_self("chitkul lakshya"));
        assert!(sn.is_self("Chitkul"));
        assert!(sn.is_self("lakshya"));
        assert!(sn.is_self("chitkullakshya@gmail.com"));

        // Non-self must not match
        assert!(!sn.is_self("GitHub"));
        assert!(!sn.is_self("Supabase"));
        assert!(!sn.is_self("Prem"));

        // Empty set matches nothing
        let empty = SelfNames::none();
        assert!(!empty.is_self("Chitkul"));
        assert!(!empty.is_self("chitkul lakshya"));
        assert!(!empty.is_self(""));
    }
}
