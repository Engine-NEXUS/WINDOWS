//! Platform-specific system volume control.
//!
//! Before NEXUS speaks (TTS), the system output volume is set to a
//! user-configured level (default 75%) so the user always hears NEXUS
//! at a consistent volume. After TTS completes, the original volume is
//! restored.
//!
//! - Windows: Core Audio COM `IAudioEndpointVolume`
//! - macOS: CoreAudio `AudioObjectGetPropertyData` / `AudioObjectSetPropertyData`
//! - Linux: `wpctl` → `pactl` → `amixer` shell commands

use std::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global saved baseline volume — captured before TTS, restored after.
/// Stored as bits of an f32 in an AtomicI32.
/// -1.0 (bits: 0xBF800000) means "no volume saved" (not yet in TTS mode).
static SAVED_VOLUME: AtomicI32 = AtomicI32::new((-1.0f32).to_bits() as i32);

/// Active concurrent TTS speaker count.
/// Volume is only restored to the user baseline when this counter drops back to 0.
static ACTIVE_TTS_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Timestamp in milliseconds of the last volume adjustment.
static LAST_TTS_TIMESTAMP_MS: AtomicU64 = AtomicU64::new(0);

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn load_saved_volume() -> f32 {
    f32::from_bits(SAVED_VOLUME.load(Ordering::SeqCst) as u32)
}

fn store_saved_volume(vol: f32) {
    SAVED_VOLUME.store(vol.to_bits() as i32, Ordering::SeqCst);
}

/// RAII Lease: Automatically decrements the active speech counter and restores
/// volume when dropped (e.g. on early task abort, error, or completion).
pub struct TtsVolumeLease {
    active: bool,
}

impl TtsVolumeLease {
    pub fn new() -> Self {
        Self { active: true }
    }
}

impl Drop for TtsVolumeLease {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            release_volume_lease();
        }
    }
}

/// Acquire an RAII lease for TTS volume.
/// Saves the user baseline if transitioning from 0 speakers, sets system volume to `tts_volume`,
/// and returns an RAII lease that automatically restores volume when dropped.
pub fn acquire_volume_lease(tts_volume: f32) -> Option<TtsVolumeLease> {
    let tts_volume = tts_volume.clamp(0.0, 1.0);

    // Watchdog check: if active for >15s without clearing, force-drain first
    check_volume_watchdog();

    let prev_count = ACTIVE_TTS_COUNT.fetch_add(1, Ordering::SeqCst);
    LAST_TTS_TIMESTAMP_MS.store(now_millis(), Ordering::SeqCst);

    if prev_count == 0 {
        // Transitioning from 0 to 1 active speaker: record baseline volume
        let current = get_system_volume().unwrap_or(-1.0);
        let existing_saved = load_saved_volume();

        // Sanity check: If current volume is already at tts_volume (within 2%),
        // do not overwrite an existing valid saved baseline.
        let is_already_target = current >= 0.0 && (current - tts_volume).abs() < 0.025;
        if is_already_target && existing_saved >= 0.0 {
            tracing::info!(
                "volume: current volume {:.2} is already target; preserving existing baseline {:.2}",
                current,
                existing_saved
            );
        } else if current >= 0.0 {
            store_saved_volume(current);
            tracing::info!("volume: saved baseline {:.2}, setting to {:.2} for TTS", current, tts_volume);
        } else if existing_saved < 0.0 {
            tracing::warn!("volume: failed to read current volume, using fallback baseline 0.30");
            store_saved_volume(0.30);
        }

        if let Err(e) = set_system_volume(tts_volume) {
            tracing::warn!("volume: failed to set TTS volume: {e}");
            ACTIVE_TTS_COUNT.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
    } else {
        tracing::debug!(
            "volume: reentrant TTS speaker (active={}), keeping baseline {:.2}",
            prev_count + 1,
            load_saved_volume()
        );
        let _ = set_system_volume(tts_volume);
    }

    Some(TtsVolumeLease::new())
}

/// Release a lease. When the last active speaker finishes, restores the original volume.
pub fn release_volume_lease() {
    let prev = ACTIVE_TTS_COUNT.fetch_sub(1, Ordering::SeqCst);
    if prev <= 1 {
        // We were the last active speaker: restore to baseline
        ACTIVE_TTS_COUNT.store(0, Ordering::SeqCst);
        let saved = load_saved_volume();
        if saved >= 0.0 {
            tracing::info!("volume: all TTS finished, restoring to baseline {:.2}", saved);
            let _ = set_system_volume(saved);
        }
        store_saved_volume(-1.0);
    } else {
        tracing::debug!("volume: utterance completed, {} active speakers remaining", prev - 1);
    }
}

/// Force immediate volume restoration (used on barge-in, explicit stop, or watchdog expiration).
pub fn force_restore_volume() {
    ACTIVE_TTS_COUNT.store(0, Ordering::SeqCst);
    let saved = load_saved_volume();
    if saved >= 0.0 {
        tracing::info!("volume: force restoring to baseline {:.2}", saved);
        let _ = set_system_volume(saved);
    }
    store_saved_volume(-1.0);
}

/// Failsafe watchdog: If active speech counter has been non-zero for over 15 seconds,
/// force restore to prevent any perpetual elevation.
pub fn check_volume_watchdog() {
    let count = ACTIVE_TTS_COUNT.load(Ordering::SeqCst);
    if count > 0 {
        let last_time = LAST_TTS_TIMESTAMP_MS.load(Ordering::SeqCst);
        let elapsed = now_millis().saturating_sub(last_time);
        if elapsed > 15_000 {
            tracing::warn!("volume: watchdog timeout ({}ms), force-draining volume", elapsed);
            force_restore_volume();
        }
    }
}

/// Backward compatibility: Save system volume and set to `tts_volume`.
pub fn save_and_set_volume(tts_volume: f32) -> bool {
    acquire_volume_lease(tts_volume).is_some()
}

/// Backward compatibility: Restore system volume.
pub fn restore_volume() {
    release_volume_lease();
}

// ─── Windows: Core Audio COM IAudioEndpointVolume ──────────────────────

#[cfg(target_os = "windows")]
pub fn get_system_volume() -> Result<f32, String> {
    use windows::Win32::Media::Audio::{
        eRender, eMultimedia, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::core::Interface;

    unsafe {
        // Ensure COM is initialized on this thread (safe to call repeatedly)
        let _ = CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("volume: CoCreateInstance failed: {e}"))?;

        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .map_err(|e| format!("volume: GetDefaultAudioEndpoint failed: {e}"))?;

        // windows 0.36 uses raw Activate (same pattern as meeting_detect.rs)
        let iid = IAudioEndpointVolume::IID;
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        device
            .Activate(&iid, CLSCTX_ALL, std::ptr::null(), &mut ptr as *mut _)
            .map_err(|e| format!("volume: Activate IAudioEndpointVolume failed: {e}"))?;
        let endpoint_volume: IAudioEndpointVolume = std::mem::transmute(ptr);

        let level = endpoint_volume
            .GetMasterVolumeLevelScalar()
            .map_err(|e| format!("volume: GetMasterVolumeLevelScalar failed: {e}"))?;

        Ok(level)
    }
}

#[cfg(target_os = "windows")]
pub fn set_system_volume(level: f32) -> Result<(), String> {
    use windows::Win32::Media::Audio::{
        eRender, eMultimedia, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::core::{GUID, Interface};

    let level = level.clamp(0.0, 1.0);

    unsafe {
        let _ = CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("volume: CoCreateInstance failed: {e}"))?;

        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .map_err(|e| format!("volume: GetDefaultAudioEndpoint failed: {e}"))?;

        let iid = IAudioEndpointVolume::IID;
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        device
            .Activate(&iid, CLSCTX_ALL, std::ptr::null(), &mut ptr as *mut _)
            .map_err(|e| format!("volume: Activate IAudioEndpointVolume failed: {e}"))?;
        let endpoint_volume: IAudioEndpointVolume = std::mem::transmute(ptr);

        endpoint_volume
            .SetMasterVolumeLevelScalar(level, &GUID::zeroed())
            .map_err(|e| format!("volume: SetMasterVolumeLevelScalar failed: {e}"))?;

        Ok(())
    }
}

// ─── macOS: CoreAudio FFI ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[repr(C)]
struct AudioObjectPropertyAddress {
    mSelector: u32,
    mScope: u32,
    mElement: u32,
}

#[cfg(target_os = "macos")]
extern "C" {
    fn AudioObjectGetPropertyData(
        inObjectID: u32,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const std::ffi::c_void,
        ioDataSize: *mut u32,
        outData: *mut std::ffi::c_void,
    ) -> i32;

    fn AudioObjectSetPropertyData(
        inObjectID: u32,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const std::ffi::c_void,
        inDataSize: u32,
        inData: *const std::ffi::c_void,
    ) -> i32;
}

#[cfg(target_os = "macos")]
mod consts {
    pub const K_AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
    pub const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    pub const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MASTER: u32 = 0;
    pub const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = u32::from_be_bytes(*b"dOut");
    pub const K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MASTER_VOLUME: u32 =
        u32::from_be_bytes(*b"vmvc");
    pub const K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
}

#[cfg(target_os = "macos")]
fn get_default_output_device() -> Result<u32, String> {
    use consts::*;

    unsafe {
        let mut device_id: u32 = 0;
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        let addr = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MASTER,
        };
        let status = AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut device_id as *mut _ as *mut _,
        );
        if status != 0 {
            return Err(format!("volume: GetDefaultOutputDevice failed: {status}"));
        }
        Ok(device_id)
    }
}

#[cfg(target_os = "macos")]
pub fn get_system_volume() -> Result<f32, String> {
    use consts::*;

    unsafe {
        let device_id = get_default_output_device()?;
        let mut volume: f32 = 0.0;
        let mut size: u32 = std::mem::size_of::<f32>() as u32;
        let addr = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MASTER_VOLUME,
            mScope: K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MASTER,
        };
        let status = AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut volume as *mut _ as *mut _,
        );
        if status != 0 {
            return Err(format!("volume: GetVolume failed: {status}"));
        }
        Ok(volume)
    }
}

#[cfg(target_os = "macos")]
pub fn set_system_volume(level: f32) -> Result<(), String> {
    use consts::*;

    let level = level.clamp(0.0, 1.0);

    unsafe {
        let device_id = get_default_output_device()?;
        let vol = level;
        let addr = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MASTER_VOLUME,
            mScope: K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MASTER,
        };
        let status = AudioObjectSetPropertyData(
            device_id,
            &addr,
            0,
            std::ptr::null(),
            std::mem::size_of::<f32>() as u32,
            &vol as *const f32 as *const _,
        );
        if status != 0 {
            return Err(format!("volume: SetVolume failed: {status}"));
        }
        Ok(())
    }
}

// ─── Linux: wpctl → pactl → amixer shell commands ─────────────────────

#[cfg(target_os = "linux")]
pub fn get_system_volume() -> Result<f32, String> {
    use std::process::Command;

    // Try wpctl (PipeWire/WirePlumber)
    if let Ok(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_SINK@"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output: "Volume: 0.50" or "Volume: 0.50 [MUTED]"
        if let Some(line) = stdout.lines().next() {
            if let Some(vol_str) = line.split_whitespace().nth(1) {
                if let Ok(vol) = vol_str.parse::<f32>() {
                    return Ok(vol);
                }
            }
        }
    }

    // Fall back to pactl (PulseAudio)
    if let Ok(output) = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output: "Volume: front-left: 32768 /  50% / -6.02 dB,   front-right: ..."
        // Parse the first percentage
        if let Some(idx) = stdout.find('%') {
            let before = &stdout[..idx];
            if let Some(num_str) = before.rsplit(' ').next() {
                if let Ok(pct) = num_str.parse::<f32>() {
                    return Ok(pct / 100.0);
                }
            }
        }
    }

    // Fall back to amixer (ALSA)
    if let Ok(output) = Command::new("amixer").args(["get", "Master"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(idx) = stdout.find('%') {
            let before = &stdout[..idx];
            if let Some(num_str) = before.rsplit(' ').next() {
                if let Ok(pct) = num_str.parse::<f32>() {
                    return Ok(pct / 100.0);
                }
            }
        }
    }

    Err("volume: all Linux volume tools failed".to_string())
}

#[cfg(target_os = "linux")]
pub fn set_system_volume(level: f32) -> Result<(), String> {
    use std::process::Command;

    let level = level.clamp(0.0, 1.0);
    let pct = (level * 100.0).round() as i32;
    let pct_str = format!("{}%", pct);

    // Try wpctl → pactl → amixer
    let result = Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_SINK@", &pct_str])
        .status()
        .or_else(|_| {
            Command::new("pactl")
                .args(["set-sink-volume", "@DEFAULT_SINK@", &pct_str])
                .status()
        })
        .or_else(|_| {
            Command::new("amixer")
                .args(["-q", "set", "Master", &pct_str])
                .status()
        });

    match result {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("volume: set command exited with {s}")),
        Err(e) => Err(format!("volume: all Linux volume tools failed: {e}")),
    }
}

// ─── Unsupported platforms ─────────────────────────────────────────────

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn get_system_volume() -> Result<f32, String> {
    Err("volume: unsupported platform".to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn set_system_volume(_level: f32) -> Result<(), String> {
    Err("volume: unsupported platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_roundtrip() {
        let vol = get_system_volume();
        println!("CURRENT SYSTEM VOLUME: {:?}", vol);
        if let Ok(v) = vol {
            assert!(v >= 0.0 && v <= 1.0);
        }
    }

    #[test]
    fn test_save_and_restore() {
        let initial = get_system_volume().expect("get initial volume");
        println!("Initial: {:.2}", initial);
        let ok = save_and_set_volume(0.70);
        println!("save_and_set_volume ok: {}", ok);
        let during = get_system_volume().expect("get during volume");
        println!("During: {:.2}", during);
        restore_volume();
        let after = get_system_volume().expect("get after volume");
        println!("After: {:.2}", after);
        assert!((after - initial).abs() < 0.02, "Volume should be restored to initial! Expected {:.2}, got {:.2}", initial, after);
    }
}
