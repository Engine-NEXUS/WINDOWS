//! At-rest encryption for the Memory Core (plan P2).
//!
//! The whole SQLite database (records, FTS index, audit, egress log) lives in
//! memory and is persisted as ONE sealed blob: `NXM1 | nonce(12) | AES-256-GCM
//! ciphertext+tag`. Encrypting the file as a unit — rather than individual
//! columns — is deliberate: an FTS index over plaintext columns would leak the
//! very text the columns hide.
//!
//! The 256-bit data key lives in Windows Credential Manager (keyring with the
//! `windows-native` backend, DPAPI-protected per user). Honest scope: this
//! stops disk theft, other OS users and casual file reads. It does NOT stop
//! malware running as the same user, who can ask the same API for the key.

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};

pub const MAGIC: &[u8; 4] = b"NXM1";
pub const KEY_LEN: usize = 32;
pub type Key = [u8; KEY_LEN];

const KEYRING_SERVICE: &str = "com.nexus.assistant";
#[allow(dead_code)]
const KEYRING_USER: &str = "memcore_data_key_v1";

pub fn generate_key() -> Option<Key> {
    let mut k = [0u8; KEY_LEN];
    SystemRandom::new().fill(&mut k).ok()?;
    Some(k)
}

pub fn is_sealed(blob: &[u8]) -> bool {
    blob.len() >= MAGIC.len() + NONCE_LEN + 16 && &blob[..MAGIC.len()] == MAGIC
}

/// Encrypt `plaintext` with a fresh random nonce. Header is bound as AAD so
/// a tampered/relabelled blob fails authentication.
pub fn seal(key: &Key, plaintext: &[u8]) -> Option<Vec<u8>> {
    let sealing = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).ok()?);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce_bytes).ok()?;
    let mut buf = plaintext.to_vec();
    sealing
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(MAGIC.as_slice()),
            &mut buf,
        )
        .ok()?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + buf.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&buf);
    Some(out)
}

/// Decrypt a blob produced by `seal`. `None` = wrong key, tampering or truncation.
pub fn open(key: &Key, blob: &[u8]) -> Option<Vec<u8>> {
    if !is_sealed(blob) {
        return None;
    }
    let opening = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).ok()?);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&blob[MAGIC.len()..MAGIC.len() + NONCE_LEN]);
    let mut buf = blob[MAGIC.len() + NONCE_LEN..].to_vec();
    let plain = opening
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(MAGIC.as_slice()),
            &mut buf,
        )
        .ok()?;
    Some(plain.to_vec())
}

fn to_hex(k: &Key) -> String {
    k.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Key> {
    let s = s.trim();
    if s.len() != KEY_LEN * 2 {
        return None;
    }
    let mut k = [0u8; KEY_LEN];
    for (i, byte) in k.iter_mut().enumerate() {
        *byte = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(k)
}

#[allow(dead_code)]
fn read_keyring() -> Option<Key> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?;
    from_hex(&entry.get_password().ok()?)
}

/// Store `key` and prove it persisted by reading it back through a FRESH
/// entry handle. keyring's mock backend (no platform feature) fails this
/// check, so a non-persistent key store can never silently own your data.
#[allow(dead_code)]
fn store_keyring(key: &Key) -> bool {
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) else {
        return false;
    };
    if entry.set_password(&to_hex(key)).is_err() {
        return false;
    }
    read_keyring().as_ref() == Some(key)
}

/// Existing data key, or a new one that verifiably persisted. `None` means
/// the OS key store is unusable — callers then keep the database unencrypted
/// rather than encrypt with a key that could be lost.
#[allow(dead_code)]
pub fn load_or_create_key() -> Option<Key> {
    // A key is created ONLY when the store says there is no entry at all. Any other error (the key store busy or not yet
    // ready at logon) is retried a few times and then reported as "no key": creating a new key over a real one makes the
    // sealed memory unreadable for good.
    for attempt in 0..8 {
        match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map(|e| e.get_password()) {
            Ok(Ok(pw)) => {
                if let Some(k) = from_hex(&pw) {
                    return Some(k);
                }
                tracing::warn!("memcore: the stored data key is not valid; not replacing it");
                return None;
            }
            Ok(Err(keyring::Error::NoEntry)) => break,
            _ => {
                if attempt == 7 {
                    tracing::warn!("memcore: the OS key store could not be read; memory stays closed this run");
                    return None;
                }
                // ~5 s in all: Credential Manager can be slow right after logon.
                std::thread::sleep(std::time::Duration::from_millis(750));
            }
        }
    }
    // The store answered "no entry": the key is really gone (Credential Manager was cleared). A sealed file that exists can
    // then never be opened; keep it aside (never deleted) so memory can start fresh instead of staying closed forever.
    if let Some(dir) = crate::app_dirs::nexus_data_dir() {
        let enc = crate::memory::resolve_memory_dir(&dir).join("memcore.enc");
        if enc.exists() {
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let aside = enc.with_file_name(format!("memcore.enc.unreadable-{ts}"));
            if std::fs::rename(&enc, &aside).is_err() {
                tracing::warn!("memcore: no data key and the sealed memory could not be set aside; memory stays closed");
                return None;
            }
            tracing::warn!("memcore: the data key is gone; the old sealed memory was kept as {}", aside.display());
        }
    }
    let k = generate_key()?;
    if store_keyring(&k) {
        Some(k)
    } else {
        tracing::warn!("memcore: OS key store did not persist the data key; at-rest encryption disabled");
        None
    }
}

/// New data key for crypto-erase on wipe, committed safely.
///
/// The key in the OS store and the key the sealed file was written with must never drift apart: a file sealed with a key
/// that is no longer stored can not be opened on the next start. Order: store the new key, then let `commit` re-seal the
/// file with it. If `commit` reports failure the OLD key is put back, so the file on disk (still sealed with the old
/// key) stays readable. On success the process-wide key is updated too, so every other sealed file written later in
/// this run uses the key the next start will find.
#[allow(dead_code)]
pub fn rotate_key_with(commit: impl FnOnce(&Key) -> bool) -> Option<Key> {
    let old = read_keyring();
    let k = generate_key()?;
    if !store_keyring(&k) {
        // The store may hold a half-written value; put the old key back if there was one.
        if let Some(o) = old {
            let _ = store_keyring(&o);
        }
        return None;
    }
    if !commit(&k) {
        match old {
            Some(o) if store_keyring(&o) => {
                tracing::warn!("memcore: the re-sealed memory could not be written; kept the previous data key");
            }
            _ => tracing::warn!("memcore: the re-sealed memory could not be written and the previous data key could not be restored"),
        }
        return None;
    }
    #[cfg(not(test))]
    {
        *PROCESS_KEY.lock() = Some(k);
    }
    Some(k)
}

#[cfg(not(test))]
static PROCESS_KEY: once_cell::sync::Lazy<parking_lot::Mutex<Option<Key>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(load_or_create_key()));

/// Process-wide key (resolved once, replaced only by a committed rotation).
/// Tests get a fixed key so they never touch Credential Manager.
pub fn process_key() -> Option<Key> {
    #[cfg(test)]
    {
        Some([7u8; KEY_LEN])
    }
    #[cfg(not(test))]
    {
        *PROCESS_KEY.lock()
    }
}

/// Overwrite a file with zeros, then delete it (best effort; SSD wear
/// levelling means this is hygiene, not a guarantee — the key rotation on
/// wipe is what actually makes old bytes useless).
pub fn shred_file(path: &std::path::Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(path) {
            use std::io::Write;
            let zeros = vec![0u8; 8192];
            let mut left = meta.len();
            while left > 0 {
                let n = left.min(zeros.len() as u64) as usize;
                if f.write_all(&zeros[..n]).is_err() {
                    break;
                }
                left -= n as u64;
            }
            let _ = f.sync_all();
        }
    }
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trip_and_fresh_nonce() {
        let key = generate_key().unwrap();
        let a = seal(&key, b"my dog is Bruno").unwrap();
        let b = seal(&key, b"my dog is Bruno").unwrap();
        assert_ne!(a, b, "nonce must differ per seal");
        assert!(is_sealed(&a));
        assert!(!a.windows(5).any(|w| w == b"Bruno"), "plaintext leaked into ciphertext");
        assert_eq!(open(&key, &a).unwrap(), b"my dog is Bruno");
    }

    #[test]
    fn wrong_key_tamper_and_truncation_fail() {
        let key = generate_key().unwrap();
        let other = generate_key().unwrap();
        let blob = seal(&key, b"secret").unwrap();
        assert!(open(&other, &blob).is_none());
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(open(&key, &tampered).is_none());
        assert!(open(&key, &blob[..blob.len() - 3]).is_none());
        assert!(open(&key, b"NXM1").is_none());
        assert!(open(&key, b"plain sqlite bytes that are long enough to pass length").is_none());
    }

    #[test]
    fn hex_round_trip_rejects_garbage() {
        let k = generate_key().unwrap();
        assert_eq!(from_hex(&to_hex(&k)), Some(k));
        assert!(from_hex("zz").is_none());
        assert!(from_hex(&"g".repeat(64)).is_none());
    }

    #[test]
    fn shred_removes_file() {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_shred_{}", std::process::id()));
        std::fs::write(&p, vec![9u8; 20_000]).unwrap();
        shred_file(&p);
        assert!(!p.exists());
    }

    /// Live probe (run by hand, two separate processes):
    ///   cargo test --lib live_keyring_write -- --ignored
    ///   cargo test --lib live_keyring_read_and_delete -- --ignored
    /// Proves the Windows Credential Manager backend persists across processes.
    #[test]
    #[ignore = "touches the real OS credential store"]
    fn live_keyring_write() {
        let e = keyring::Entry::new(KEYRING_SERVICE, "memcore_probe_test").unwrap();
        e.set_password("probe-value-123").unwrap();
    }

    #[test]
    #[ignore = "touches the real OS credential store"]
    fn live_keyring_read_and_delete() {
        let e = keyring::Entry::new(KEYRING_SERVICE, "memcore_probe_test").unwrap();
        assert_eq!(e.get_password().unwrap(), "probe-value-123");
        e.delete_credential().unwrap();
    }
}
