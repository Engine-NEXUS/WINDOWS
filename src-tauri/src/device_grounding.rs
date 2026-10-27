//! Scope device questions before a general chat model sees them. The model
//! must not invent facts about the user's foreground window or local data.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceQuestion { Screen, ForegroundApp, ForegroundRepo, File, Account }

pub async fn live_github_login() -> Result<String, String> {
    let (worker_url, user_id, _) = crate::network::get_session_info()
        .ok_or("NEXUS has no connected session")?;
    let token = crate::github_cmd::get_github_token(&worker_url, &user_id).await
        .map_err(|_| "GitHub connection is unavailable")?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build().map_err(|_| "GitHub request could not start")?;
    let response = client.get("https://api.github.com/user")
        .header("User-Agent", "nexus-assistant")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token).send().await
        .map_err(|_| "GitHub did not respond")?;
    if !response.status().is_success() { return Err("GitHub did not verify the account".into()); }
    let json: serde_json::Value = response.json().await.map_err(|_| "GitHub response was unreadable")?;
    let login = json.get("login").and_then(|v| v.as_str()).unwrap_or("").trim();
    if login.is_empty() { return Err("GitHub response did not include an account name".into()); }
    Ok(login.to_string())
}

pub fn classify(text: &str) -> Option<DeviceQuestion> {
    let t = text.trim().to_ascii_lowercase();
    let question = ["what ", "what's ", "which ", "who ", "where ", "why ", "how ", "tell me ", "explain ", "describe ", "read ", "summarize ", "summarise "]
        .iter().any(|p| t.starts_with(p));
    if !question { return None; }
    if ["my screen", "this screen", "on screen", "on my screen", "i'm seeing", "i am seeing", "what i see", "this window", "this page", "this file"].iter().any(|p| t.contains(p)) {
        return Some(DeviceQuestion::Screen);
    }
    if ["current app", "foreground app", "app am i using", "application am i using", "app is open", "active app"].iter().any(|p| t.contains(p)) {
        return Some(DeviceQuestion::ForegroundApp);
    }
    if ["current repo", "open repo", "repo am i", "repository am i", "this repo", "my repo", "my repository", " in the repo", " in the repository", " about the repo"].iter().any(|p| t.contains(p)) {
        return Some(DeviceQuestion::ForegroundRepo);
    }
    let named_file = t.split_whitespace().map(|s| s.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_'))
        .any(|s| [".md", ".txt", ".pdf", ".docx", ".xlsx", ".json", ".rs", ".tsx", ".ts", ".py", ".toml"].iter().any(|ext| s.ends_with(ext)));
    if named_file || ["my file", "file on my laptop", "file on this laptop", "my document"].iter().any(|p| t.contains(p)) {
        return Some(DeviceQuestion::File);
    }
    if (t.contains("account") && t.contains("connected"))
        || ["my github account", "my connected account", "my account on this laptop"].iter().any(|p| t.contains(p)) {
        return Some(DeviceQuestion::Account);
    }
    None
}

/// An imperative that deterministic parsing missed may not be promoted by a
/// cloud classifier. Unknown ordinary questions still use general chat.
pub fn looks_like_unparsed_command(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    let t = ["please ", "nexus ", "can you ", "could you ", "would you "]
        .iter().find_map(|prefix| lower.strip_prefix(*prefix)).unwrap_or(&lower);
    ["open ", "close ", "launch ", "start ", "stop ", "send ", "delete ", "move ", "copy ", "rename ", "type ", "click ", "merge ", "approve ", "post ", "share ", "download ", "upload ", "shut down ", "restart "]
        .iter().any(|prefix| t.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_only_device_questions() {
        assert_eq!(classify("What am I seeing on my screen?"), Some(DeviceQuestion::Screen));
        assert_eq!(classify("Which app am I using?"), Some(DeviceQuestion::ForegroundApp));
        assert_eq!(classify("What is my current repo?"), Some(DeviceQuestion::ForegroundRepo));
        assert_eq!(classify("What does this file say?"), Some(DeviceQuestion::Screen));
        assert_eq!(classify("Where is my file?"), Some(DeviceQuestion::File));
        assert_eq!(classify("What's in README.md?"), Some(DeviceQuestion::File));
        assert_eq!(classify("Which account is connected?"), Some(DeviceQuestion::Account));
        assert_eq!(classify("What is a repository?"), None);
        assert_eq!(classify("How does a screen work?"), None);
    }
    #[test]
    fn unknown_actions_cannot_go_to_chat() {
        assert!(looks_like_unparsed_command("send the thing somewhere"));
        assert!(looks_like_unparsed_command("Could you send the thing somewhere"));
        assert!(!looks_like_unparsed_command("what is the weather"));
    }
}
