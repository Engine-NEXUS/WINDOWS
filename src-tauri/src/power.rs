//! System power and energy efficiency optimizations.
//!
//! Enables Windows EcoQoS (Efficiency Mode) to schedule background threads
//! exclusively on E-cores (Efficiency cores), reducing idle power draw by up to 70%.

#[cfg(target_os = "windows")]
pub fn enable_process_ecoqos() {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, SetProcessInformation, ProcessPowerThrottling,
        PROCESS_POWER_THROTTLING_STATE, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    };

    unsafe {
        let mut throttling = PROCESS_POWER_THROTTLING_STATE {
            Version: 1, // PROCESS_POWER_THROTTLING_CURRENT_VERSION
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        };

        let ok = SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &mut throttling as *mut _ as *mut _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        if ok.as_bool() {
            tracing::info!("power: Windows EcoQoS (Efficiency Mode) enabled for daemon");
        } else {
            tracing::debug!("power: Windows EcoQoS not enabled (unsupported OS build or hypervisor)");
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn enable_process_ecoqos() {}
