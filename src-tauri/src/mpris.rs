//! Linux Native D-Bus & MPRIS Media Control + Desktop Notifications.
//! Communicates directly with the D-Bus system/session bus using `zbus`.
//! Zero-latency, zero-fork — no `pkill` or sub-shell spawning.

#[cfg(target_os = "linux")]
pub async fn send_mpris_command(command: &str) -> Result<String, String> {
    use zbus::fdo::DBusProxy;
    use zbus::Connection;

    let connection = Connection::session()
        .await
        .map_err(|e| format!("Failed to connect to D-Bus session: {e}"))?;

    let dbus_proxy = DBusProxy::new(&connection)
        .await
        .map_err(|e| format!("Failed to create D-Bus proxy: {e}"))?;
    let names = dbus_proxy
        .list_names()
        .await
        .map_err(|e| format!("Failed to list D-Bus names: {e}"))?;

    let mpris_players: Vec<String> = names
        .into_iter()
        .filter(|n| n.as_str().starts_with("org.mpris.MediaPlayer2."))
        .map(|n| n.to_string())
        .collect();

    if mpris_players.is_empty() {
        return Err("No active media players found on D-Bus, sir.".to_string());
    }

    let method = match command {
        "play_pause" | "toggle" => "PlayPause",
        "play" => "Play",
        "pause" => "Pause",
        "next" => "Next",
        "previous" | "prev" => "Previous",
        "stop" => "Stop",
        _ => return Err(format!("Unknown MPRIS command: {command}")),
    };

    let mut sent = 0;
    for player in &mpris_players {
        let res = connection
            .call_method(
                Some(player.as_str()),
                "/org/mpris/MediaPlayer2",
                Some("org.mpris.MediaPlayer2.Player"),
                method,
                &(),
            )
            .await;
        if res.is_ok() {
            sent += 1;
        }
    }

    if sent > 0 {
        tracing::info!("MPRIS D-Bus: sent {} to {} player(s)", method, sent);
        Ok(format!("Executed {} on active media player.", method.to_lowercase()))
    } else {
        Err(format!("Failed to send {} to media players.", method))
    }
}

#[allow(dead_code)]
#[cfg(not(target_os = "linux"))]
pub async fn send_mpris_command(_command: &str) -> Result<String, String> {
    Err("MPRIS D-Bus is only available on Linux".to_string())
}
