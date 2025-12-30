//! System interface for reading thermal and CPU information from sysfs
//!
//! This module reads directly from Linux sysfs to minimize dependencies.
//! All temperatures are in Celsius, frequencies in MHz.

use std::fs;
use std::io::{self, ErrorKind};
use std::process::Command;

/// Thermal attenuation factor for keyboard temperature estimation
/// Based on physical model: T_kbd = T_amb + (T_cpu - T_amb) * ATTENUATION
const THERMAL_ATTENUATION: f32 = 0.45;

/// Default ambient temperature when not measurable
const DEFAULT_AMBIENT: f32 = 28.0;

/// CPU mode enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Performance,
    Comfort,
    Balanced,
    Quiet,
    #[default]
    Auto,
    Unknown,
}

impl Mode {
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Performance => "PERFORMANCE",
            Mode::Comfort => "COMFORT",
            Mode::Balanced => "BALANCED",
            Mode::Quiet => "QUIET",
            Mode::Auto => "AUTO",
            Mode::Unknown => "UNKNOWN",
        }
    }

    pub fn command(&self) -> &'static str {
        match self {
            Mode::Performance => "performance",
            Mode::Comfort => "comfort",
            Mode::Balanced => "balanced",
            Mode::Quiet => "quiet",
            Mode::Auto => "auto",
            Mode::Unknown => "auto",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Mode::Performance => "100% - Video calls",
            Mode::Comfort => "60% - Cool keyboard",
            Mode::Balanced => "75% - General use",
            Mode::Quiet => "40% - Silent",
            Mode::Auto => "Automatic",
            Mode::Unknown => "Unknown",
        }
    }

    pub fn all() -> &'static [Mode] {
        &[Mode::Performance, Mode::Comfort, Mode::Balanced, Mode::Quiet, Mode::Auto]
    }
}

/// Thermal zone classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalZone {
    Cool,      // < 40°C
    Comfort,   // 40-45°C
    Optimal,   // 45-50°C
    Warm,      // 50-55°C
    Hot,       // 55-65°C
    Critical,  // > 65°C
}

impl ThermalZone {
    pub fn from_cpu_temp(temp: f32) -> Self {
        match temp {
            t if t < 40.0 => ThermalZone::Cool,
            t if t < 45.0 => ThermalZone::Comfort,
            t if t < 50.0 => ThermalZone::Optimal,
            t if t < 55.0 => ThermalZone::Warm,
            t if t < 65.0 => ThermalZone::Hot,
            _ => ThermalZone::Critical,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ThermalZone::Cool => "COOL",
            ThermalZone::Comfort => "COMFORT",
            ThermalZone::Optimal => "OPTIMAL",
            ThermalZone::Warm => "WARM",
            ThermalZone::Hot => "HOT",
            ThermalZone::Critical => "CRITICAL",
        }
    }

    /// Returns RGB color tuple
    pub fn color_rgb(&self) -> (u8, u8, u8) {
        match self {
            ThermalZone::Cool => (100, 200, 255),     // Light blue
            ThermalZone::Comfort => (100, 220, 100),  // Green
            ThermalZone::Optimal => (150, 220, 100),  // Light green
            ThermalZone::Warm => (255, 200, 100),     // Yellow
            ThermalZone::Hot => (255, 150, 100),      // Orange
            ThermalZone::Critical => (255, 100, 100), // Red
        }
    }
}

/// Read a single value from a sysfs file
fn read_sysfs_value(path: &str) -> io::Result<String> {
    fs::read_to_string(path).map(|s| s.trim().to_string())
}

/// Read CPU temperature from thermal zones
/// Tries x86_pkg_temp first, then TCPU, then any available
pub fn read_cpu_temp() -> io::Result<f32> {
    // Try known thermal zone paths
    let paths = [
        "/sys/class/thermal/thermal_zone10/temp", // x86_pkg_temp on IdeaPad
        "/sys/class/thermal/thermal_zone8/temp",  // TCPU
        "/sys/class/thermal/thermal_zone0/temp",  // fallback
    ];

    for path in paths {
        if let Ok(content) = read_sysfs_value(path) {
            if let Ok(millicelsius) = content.parse::<i32>() {
                let temp = millicelsius as f32 / 1000.0;
                if temp > 0.0 && temp < 150.0 {
                    return Ok(temp);
                }
            }
        }
    }

    // Scan all thermal zones for x86_pkg_temp or TCPU
    for i in 0..15 {
        let type_path = format!("/sys/class/thermal/thermal_zone{}/type", i);
        let temp_path = format!("/sys/class/thermal/thermal_zone{}/temp", i);

        if let Ok(zone_type) = read_sysfs_value(&type_path) {
            if zone_type == "x86_pkg_temp" || zone_type == "TCPU" {
                if let Ok(content) = read_sysfs_value(&temp_path) {
                    if let Ok(millicelsius) = content.parse::<i32>() {
                        return Ok(millicelsius as f32 / 1000.0);
                    }
                }
            }
        }
    }

    Err(io::Error::new(ErrorKind::NotFound, "No CPU temperature sensor found"))
}

/// Read ambient temperature (from ACPI thermal zone)
pub fn read_ambient_temp() -> f32 {
    // Try acpitz which usually reports chassis/ambient temp
    if let Ok(content) = read_sysfs_value("/sys/class/thermal/thermal_zone0/temp") {
        if let Ok(millicelsius) = content.parse::<i32>() {
            let temp = millicelsius as f32 / 1000.0;
            if temp > 15.0 && temp < 50.0 {
                return temp;
            }
        }
    }
    DEFAULT_AMBIENT
}

/// Calculate estimated keyboard temperature using thermal physics model
/// Formula: T_kbd = T_amb + (T_cpu - T_amb) * attenuation_factor
pub fn calculate_keyboard_temp(cpu_temp: f32, ambient_temp: f32) -> f32 {
    ambient_temp + (cpu_temp - ambient_temp) * THERMAL_ATTENUATION
}

/// Read current performance percentage from intel_pstate
pub fn read_perf_pct() -> io::Result<u8> {
    let content = read_sysfs_value("/sys/devices/system/cpu/intel_pstate/max_perf_pct")?;
    content.parse::<u8>().map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
}

/// Read current CPU frequency in MHz
pub fn read_current_freq() -> io::Result<u32> {
    let content = read_sysfs_value("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")?;
    let khz: u32 = content.parse().map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
    Ok(khz / 1000)
}

/// Read maximum CPU frequency in MHz
pub fn read_max_freq() -> io::Result<u32> {
    let content = read_sysfs_value("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")?;
    let khz: u32 = content.parse().map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
    Ok(khz / 1000)
}

/// Read current mode from cpu-mode status file
pub fn read_mode() -> Mode {
    if let Ok(content) = read_sysfs_value("/tmp/cpu-mode.current") {
        let lower = content.to_lowercase();
        if lower.contains("performance") {
            Mode::Performance
        } else if lower.contains("comfort") {
            if lower.contains("auto") || lower.contains("-") {
                Mode::Auto // comfort-OPTIMAL, etc.
            } else {
                Mode::Comfort
            }
        } else if lower.contains("balanced") {
            Mode::Balanced
        } else if lower.contains("quiet") {
            Mode::Quiet
        } else if lower.contains("auto") {
            Mode::Auto
        } else {
            Mode::Unknown
        }
    } else {
        Mode::Unknown
    }
}

/// Read platform profile
pub fn read_platform_profile() -> String {
    read_sysfs_value("/sys/firmware/acpi/platform_profile").unwrap_or_else(|_| "unknown".into())
}

/// Read fan mode (0=auto, 1=boost)
pub fn read_fan_mode() -> u8 {
    read_sysfs_value("/sys/devices/pci0000:00/0000:00:1f.0/PNP0C09:00/VPC2004:00/fan_mode")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Activate fan boost (max speed) - Lenovo IdeaPad specific
pub fn set_fan_boost(enable: bool) -> io::Result<()> {
    let value = if enable { "1" } else { "0" };
    let output = Command::new("pkexec")
        .args(["bash", "-c", &format!(
            "echo {} > /sys/devices/pci0000:00/0000:00:1f.0/PNP0C09:00/VPC2004:00/fan_mode",
            value
        )])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::new(ErrorKind::Other, "Failed to set fan mode"))
    }
}

/// Set performance percentage directly
pub fn set_perf_pct(pct: u8) -> io::Result<()> {
    let pct = pct.clamp(20, 100);
    let output = Command::new("pkexec")
        .args(["bash", "-c", &format!(
            "echo {} > /sys/devices/system/cpu/intel_pstate/max_perf_pct 2>/dev/null || \
             echo {} > /sys/devices/system/cpu/amd_pstate/max_perf_pct 2>/dev/null || \
             for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_max_freq; do \
               max=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq); \
               echo $((max * {} / 100)) > $cpu; \
             done",
            pct, pct, pct
        )])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::new(ErrorKind::Other, "Failed to set performance"))
    }
}

/// Calculate required performance percentage to reach target temperature
pub fn calc_perf_for_target(current_temp: f32, target_temp: f32, current_perf: u8) -> u8 {
    if current_temp <= target_temp {
        // Below target, can increase performance
        (current_perf as f32 * 1.1).min(100.0) as u8
    } else {
        // Above target, reduce performance proportionally
        let ratio = target_temp / current_temp;
        ((current_perf as f32 * ratio) as u8).clamp(20, 100)
    }
}

/// Apply thermal control to reach target temperature
pub fn apply_thermal_control(current_temp: f32, target_temp: f32) -> io::Result<String> {
    let current_perf = read_perf_pct().unwrap_or(75);
    let diff = current_temp - target_temp;

    if diff > 10.0 {
        // Critical: fan boost + aggressive throttle
        let _ = set_fan_boost(true);
        set_perf_pct(30)?;
        Ok("CRITICAL: Fan boost + 30%".into())
    } else if diff > 5.0 {
        // High: fan boost + moderate throttle
        let _ = set_fan_boost(true);
        set_perf_pct(50)?;
        Ok("HIGH: Fan boost + 50%".into())
    } else if diff > 0.0 {
        // Slight overshoot: gradual reduction
        let new_perf = calc_perf_for_target(current_temp, target_temp, current_perf);
        set_perf_pct(new_perf)?;
        Ok(format!("Adjusting to {}%", new_perf))
    } else if diff < -5.0 {
        // Well below target: can increase
        let new_perf = (current_perf + 10).min(100);
        set_perf_pct(new_perf)?;
        Ok(format!("Increasing to {}%", new_perf))
    } else {
        Ok("On target".into())
    }
}

/// Change CPU mode using pkexec
pub fn set_mode(mode: Mode) -> io::Result<()> {
    let output = Command::new("pkexec")
        .args(["/usr/local/bin/cpu-mode", mode.command()])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(io::Error::new(ErrorKind::Other, format!("Failed to change mode: {}", stderr)))
    }
}

/// Complete thermal state snapshot
#[derive(Debug, Clone, Default)]
pub struct ThermalState {
    pub cpu_temp: f32,
    pub keyboard_temp: f32,
    pub ambient_temp: f32,
    pub perf_pct: u8,
    pub current_freq_mhz: u32,
    pub max_freq_mhz: u32,
    pub mode: Mode,
    pub platform_profile: String,
    pub fan_boost: bool,
}

impl ThermalState {
    /// Read complete thermal state from system
    pub fn read() -> Self {
        let cpu_temp = read_cpu_temp().unwrap_or(50.0);
        let ambient_temp = read_ambient_temp();
        let keyboard_temp = calculate_keyboard_temp(cpu_temp, ambient_temp);

        Self {
            cpu_temp,
            keyboard_temp,
            ambient_temp,
            perf_pct: read_perf_pct().unwrap_or(50),
            current_freq_mhz: read_current_freq().unwrap_or(1000),
            max_freq_mhz: read_max_freq().unwrap_or(4400),
            mode: read_mode(),
            platform_profile: read_platform_profile(),
            fan_boost: read_fan_mode() == 1,
        }
    }

    /// Get thermal zone classification
    pub fn thermal_zone(&self) -> ThermalZone {
        ThermalZone::from_cpu_temp(self.cpu_temp)
    }

    /// Get current frequency in GHz
    pub fn current_freq_ghz(&self) -> f32 {
        self.current_freq_mhz as f32 / 1000.0
    }

    /// Get max frequency in GHz
    pub fn max_freq_ghz(&self) -> f32 {
        self.max_freq_mhz as f32 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thermal_zone_classification() {
        assert_eq!(ThermalZone::from_cpu_temp(35.0), ThermalZone::Cool);
        assert_eq!(ThermalZone::from_cpu_temp(42.0), ThermalZone::Comfort);
        assert_eq!(ThermalZone::from_cpu_temp(47.0), ThermalZone::Optimal);
        assert_eq!(ThermalZone::from_cpu_temp(52.0), ThermalZone::Warm);
        assert_eq!(ThermalZone::from_cpu_temp(60.0), ThermalZone::Hot);
        assert_eq!(ThermalZone::from_cpu_temp(70.0), ThermalZone::Critical);
    }

    #[test]
    fn test_thermal_zone_boundary_values() {
        assert_eq!(ThermalZone::from_cpu_temp(39.9), ThermalZone::Cool);
        assert_eq!(ThermalZone::from_cpu_temp(40.0), ThermalZone::Comfort);
        assert_eq!(ThermalZone::from_cpu_temp(44.9), ThermalZone::Comfort);
        assert_eq!(ThermalZone::from_cpu_temp(45.0), ThermalZone::Optimal);
        assert_eq!(ThermalZone::from_cpu_temp(54.9), ThermalZone::Warm);
        assert_eq!(ThermalZone::from_cpu_temp(55.0), ThermalZone::Hot);
        assert_eq!(ThermalZone::from_cpu_temp(64.9), ThermalZone::Hot);
        assert_eq!(ThermalZone::from_cpu_temp(65.0), ThermalZone::Critical);
    }

    #[test]
    fn test_thermal_zone_labels() {
        assert_eq!(ThermalZone::Cool.label(), "COOL");
        assert_eq!(ThermalZone::Comfort.label(), "COMFORT");
        assert_eq!(ThermalZone::Optimal.label(), "OPTIMAL");
        assert_eq!(ThermalZone::Warm.label(), "WARM");
        assert_eq!(ThermalZone::Hot.label(), "HOT");
        assert_eq!(ThermalZone::Critical.label(), "CRITICAL");
    }

    #[test]
    fn test_keyboard_temp_calculation() {
        // At 50°C CPU with 28°C ambient: 28 + (50-28)*0.45 = 28 + 9.9 = 37.9
        let kbd = calculate_keyboard_temp(50.0, 28.0);
        assert!((kbd - 37.9).abs() < 0.1);

        // At ambient temp, keyboard should be at ambient
        let kbd = calculate_keyboard_temp(28.0, 28.0);
        assert!((kbd - 28.0).abs() < 0.1);

        // High CPU temp
        let kbd = calculate_keyboard_temp(80.0, 25.0);
        assert!((kbd - 49.75).abs() < 0.1); // 25 + (80-25)*0.45 = 49.75

        // Low ambient
        let kbd = calculate_keyboard_temp(60.0, 20.0);
        assert!((kbd - 38.0).abs() < 0.1); // 20 + (60-20)*0.45 = 38.0
    }

    #[test]
    fn test_mode_properties() {
        assert_eq!(Mode::Performance.command(), "performance");
        assert_eq!(Mode::Comfort.label(), "COMFORT");
        assert_eq!(Mode::all().len(), 5);
    }

    #[test]
    fn test_mode_all_variants() {
        assert_eq!(Mode::Performance.command(), "performance");
        assert_eq!(Mode::Comfort.command(), "comfort");
        assert_eq!(Mode::Balanced.command(), "balanced");
        assert_eq!(Mode::Quiet.command(), "quiet");
        assert_eq!(Mode::Auto.command(), "auto");
        assert_eq!(Mode::Unknown.command(), "auto");
    }

    #[test]
    fn test_mode_labels() {
        assert_eq!(Mode::Performance.label(), "PERFORMANCE");
        assert_eq!(Mode::Comfort.label(), "COMFORT");
        assert_eq!(Mode::Balanced.label(), "BALANCED");
        assert_eq!(Mode::Quiet.label(), "QUIET");
        assert_eq!(Mode::Auto.label(), "AUTO");
        assert_eq!(Mode::Unknown.label(), "UNKNOWN");
    }

    #[test]
    fn test_mode_descriptions() {
        assert!(Mode::Performance.description().contains("100%"));
        assert!(Mode::Comfort.description().contains("60%"));
        assert!(Mode::Balanced.description().contains("75%"));
        assert!(Mode::Quiet.description().contains("40%"));
        assert!(Mode::Auto.description().contains("Automatic"));
    }

    #[test]
    fn test_thermal_zone_colors() {
        let (r, g, b) = ThermalZone::Cool.color_rgb();
        assert!(b > r); // Blue should be dominant for cool

        let (r, g, b) = ThermalZone::Critical.color_rgb();
        assert!(r > g && r > b); // Red should be dominant for critical
    }

    #[test]
    fn test_all_thermal_zone_colors_valid() {
        for zone in [
            ThermalZone::Cool,
            ThermalZone::Comfort,
            ThermalZone::Optimal,
            ThermalZone::Warm,
            ThermalZone::Hot,
            ThermalZone::Critical,
        ] {
            let (r, g, b) = zone.color_rgb();
            // All colors should have some value
            assert!(r > 0 || g > 0 || b > 0);
        }
    }

    #[test]
    fn test_calc_perf_for_target_below_target() {
        // Below target - should increase
        let perf = calc_perf_for_target(45.0, 55.0, 50);
        assert!(perf > 50); // Should increase
        assert!(perf <= 100);
    }

    #[test]
    fn test_calc_perf_for_target_above_target() {
        // Above target - should decrease
        let perf = calc_perf_for_target(60.0, 50.0, 80);
        assert!(perf < 80); // Should decrease
        assert!(perf >= 20); // Min clamp
    }

    #[test]
    fn test_calc_perf_for_target_at_target() {
        // At target - minimal change
        let perf = calc_perf_for_target(55.0, 55.0, 75);
        assert!(perf >= 20 && perf <= 100);
    }

    #[test]
    fn test_calc_perf_for_target_clamping() {
        // Very high temp - should clamp to minimum
        let perf = calc_perf_for_target(100.0, 50.0, 100);
        assert!(perf >= 20);

        // Very low temp - should clamp to maximum
        let perf = calc_perf_for_target(30.0, 80.0, 90);
        assert!(perf <= 100);
    }

    #[test]
    fn test_thermal_state_freq_conversion() {
        let state = ThermalState {
            current_freq_mhz: 2500,
            max_freq_mhz: 4400,
            ..Default::default()
        };
        assert!((state.current_freq_ghz() - 2.5).abs() < 0.01);
        assert!((state.max_freq_ghz() - 4.4).abs() < 0.01);
    }

    #[test]
    fn test_thermal_state_zone() {
        let state = ThermalState {
            cpu_temp: 45.0,
            ..Default::default()
        };
        assert_eq!(state.thermal_zone(), ThermalZone::Optimal);
    }

    #[test]
    fn test_mode_default() {
        let mode = Mode::default();
        assert_eq!(mode, Mode::Auto);
    }

    #[test]
    fn test_thermal_state_default() {
        let state = ThermalState::default();
        assert_eq!(state.cpu_temp, 0.0);
        assert_eq!(state.mode, Mode::Auto);
        assert!(!state.fan_boost);
    }
}
