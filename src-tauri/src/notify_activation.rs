//! Desktop activation routing for a notification (notification-to-sidebar N5). Pure.
//!
//! A native toast / protocol launch can only carry an opaque event id: `nexus://notify/open?event=ev_<24 hex>`.
//! Nothing else is accepted: no other host or path, no extra parameters or fragment, no encoded or non-hex ids.
//! The id is then resolved through the validated ledger like a card click, so an unknown, expired or forged
//! activation shows the read-only "unavailable" view and never opens another account, repo or the newest PR.

const PREFIX: &str = "nexus://notify/open?event=";

/// The event id of a valid activation URL, or None. A single trailing `/` (added by some launchers) is tolerated.
pub fn parse_activation(url: &str) -> Option<String> {
    let rest = url.strip_prefix(PREFIX)?;
    let id = rest.strip_suffix('/').unwrap_or(rest);
    let hex = id.strip_prefix("ev_")?;
    (hex.len() == 24 && hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))).then(|| id.to_string())
}

/// The activation URL a native toast should carry for `event_id` (None if the id is not a ledger id).
pub fn activation_url(event_id: &str) -> Option<String> {
    parse_activation(&format!("{PREFIX}{event_id}")).map(|id| format!("{PREFIX}{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ev_0123456789abcdef01234567";

    #[test]
    fn a_valid_activation_yields_exactly_the_event_id() {
        assert_eq!(parse_activation(&format!("{PREFIX}{ID}")).as_deref(), Some(ID));
        assert_eq!(parse_activation(&format!("{PREFIX}{ID}/")).as_deref(), Some(ID));
        assert_eq!(activation_url(ID), Some(format!("{PREFIX}{ID}")));
    }

    #[test]
    fn anything_else_is_rejected() {
        for bad in [
            "",
            "nexus://settings",
            "nexus://notify/open",
            "nexus://notify/open?event=",
            "nexus://notify/open?event=ev_short",
            "nexus://notify/open?event=0123456789abcdef01234567",
            "nexus://notify/open?event=ev_ev_0123456789abcdef01234567",
            "nexus://notify/open?event=ev_0123456789ABCDEF01234567",
            "nexus://notify/open?event=ev_0123456789abcdef01234567&url=https://evil.example",
            "nexus://notify/open?event=ev_0123456789abcdef01234567#frag",
            "nexus://notify/open?event=ev_0123456789abcdef0123456%37",
            "nexus://notify/open?event=ev_0123456789abcdef012345678",
            "nexus://notify/open?event=../ev_0123456789abcdef01234567",
            "nexus://notify/other?event=ev_0123456789abcdef01234567",
            "nexus://oauth/open?event=ev_0123456789abcdef01234567",
            "https://notify/open?event=ev_0123456789abcdef01234567",
            "nexus://notify/open?event=ev_0123456789abcdef01234567\n",
            "nexus://notify/open?event=ev_0123456789abcdef01234567//",
            "nexus://notify/open?event=ev_0123456789abcdef01234567 --shell",
        ] {
            assert_eq!(parse_activation(bad), None, "{bad:?}");
        }
        assert_eq!(activation_url("not-an-id"), None);
    }

    #[test]
    fn the_id_matches_what_the_ledger_generates() {
        use crate::notify_event::{entity_key, event_id, Provider, Status, Target};
        let t = Target::Repo { host: "github.com".into(), owner: "o".into(), repo: "r".into() };
        let key = entity_key(Provider::GitHub, "a", &t, "build");
        let id = event_id(&key, "12", Status::Failed);
        assert_eq!(parse_activation(&format!("{PREFIX}{id}")).as_deref(), Some(id.as_str()));
    }
}
