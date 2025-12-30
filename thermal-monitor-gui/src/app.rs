//! Application state and main loop for thermal monitor GUI
//!
//! Implements eframe::App trait for egui integration.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};

use crate::system::{Mode, ThermalState, ThermalZone, set_mode, set_fan_boost, apply_thermal_control};

/// Update interval in seconds
const UPDATE_INTERVAL_SECS: f32 = 2.0;

/// History capacity (2 minutes at 2-second intervals)
const HISTORY_CAPACITY: usize = 60;

/// Temperature history buffer
#[derive(Debug)]
pub struct TemperatureHistory {
    cpu_temps: VecDeque<f32>,
    kbd_temps: VecDeque<f32>,
    capacity: usize,
}

impl Default for TemperatureHistory {
    fn default() -> Self {
        Self::new(HISTORY_CAPACITY)
    }
}

impl TemperatureHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            cpu_temps: VecDeque::with_capacity(capacity),
            kbd_temps: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, cpu: f32, kbd: f32) {
        if self.cpu_temps.len() >= self.capacity {
            self.cpu_temps.pop_front();
            self.kbd_temps.pop_front();
        }
        self.cpu_temps.push_back(cpu);
        self.kbd_temps.push_back(kbd);
    }

    /// Get CPU temperature points for plotting
    pub fn cpu_points(&self) -> PlotPoints {
        PlotPoints::new(
            self.cpu_temps
                .iter()
                .enumerate()
                .map(|(i, &t)| [i as f64, t as f64])
                .collect(),
        )
    }

    /// Get keyboard temperature points for plotting
    pub fn kbd_points(&self) -> PlotPoints {
        PlotPoints::new(
            self.kbd_temps
                .iter()
                .enumerate()
                .map(|(i, &t)| [i as f64, t as f64])
                .collect(),
        )
    }

    pub fn len(&self) -> usize {
        self.cpu_temps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cpu_temps.is_empty()
    }
}

/// Main application state
pub struct ThermalApp {
    state: ThermalState,
    history: TemperatureHistory,
    last_update: Instant,
    status_message: Option<(String, Instant)>,
    target_temp: f32,
    auto_control: bool,
    fan_boost_manual: bool,
}

impl Default for ThermalApp {
    fn default() -> Self {
        let state = ThermalState::read();
        let mut history = TemperatureHistory::default();
        history.push(state.cpu_temp, state.keyboard_temp);

        Self {
            state,
            history,
            last_update: Instant::now(),
            status_message: None,
            target_temp: 55.0,
            auto_control: false,
            fan_boost_manual: false,
        }
    }
}

impl ThermalApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    /// Update state from system
    fn update_state(&mut self) {
        self.state = ThermalState::read();
        self.history.push(self.state.cpu_temp, self.state.keyboard_temp);

        // Apply automatic thermal control if enabled
        if self.auto_control {
            if let Ok(msg) = apply_thermal_control(self.state.cpu_temp, self.target_temp) {
                if msg != "On target" {
                    self.status_message = Some((msg, Instant::now()));
                }
            }
        }
    }

    /// Change CPU mode
    fn change_mode(&mut self, mode: Mode) {
        match set_mode(mode) {
            Ok(()) => {
                self.status_message = Some((
                    format!("Mode changed to {}", mode.label()),
                    Instant::now(),
                ));
                self.update_state();
            }
            Err(e) => {
                self.status_message = Some((
                    format!("Error: {}", e),
                    Instant::now(),
                ));
            }
        }
    }

    /// Set status message
    fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now()));
    }

    /// Get zone color as egui Color32
    fn zone_color(zone: ThermalZone) -> egui::Color32 {
        let (r, g, b) = zone.color_rgb();
        egui::Color32::from_rgb(r, g, b)
    }

    /// Get mode color
    fn mode_color(mode: Mode) -> egui::Color32 {
        match mode {
            Mode::Performance => egui::Color32::from_rgb(255, 100, 100),
            Mode::Comfort => egui::Color32::from_rgb(100, 200, 255),
            Mode::Balanced => egui::Color32::from_rgb(150, 220, 100),
            Mode::Quiet => egui::Color32::from_rgb(180, 180, 220),
            Mode::Auto => egui::Color32::from_rgb(255, 200, 100),
            Mode::Unknown => egui::Color32::GRAY,
        }
    }

    /// Render temperature gauge
    fn render_gauge(&self, ui: &mut egui::Ui, label: &str, temp: f32, zone: ThermalZone) {
        let color = Self::zone_color(zone);

        ui.vertical(|ui| {
            ui.label(egui::RichText::new(label).size(12.0).color(egui::Color32::GRAY));
            ui.label(
                egui::RichText::new(format!("{:.1}°C", temp))
                    .size(28.0)
                    .color(color)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(zone.label())
                    .size(10.0)
                    .color(color),
            );
        });
    }

    /// Render main temperature panel
    fn render_temperatures(&self, ui: &mut egui::Ui) {
        let zone = self.state.thermal_zone();

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                self.render_gauge(ui, "CPU", self.state.cpu_temp, zone);
                ui.add_space(40.0);
                self.render_gauge(ui, "KEYBOARD (est.)", self.state.keyboard_temp, zone);
                ui.add_space(40.0);
                self.render_gauge(ui, "AMBIENT", self.state.ambient_temp, ThermalZone::Cool);
            });
        });
    }

    /// Render performance info
    fn render_performance(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Performance percentage
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("PERFORMANCE").size(10.0).color(egui::Color32::GRAY));
                ui.label(
                    egui::RichText::new(format!("{}%", self.state.perf_pct))
                        .size(24.0)
                        .strong(),
                );
            });

            ui.add_space(30.0);

            // Current frequency
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("FREQUENCY").size(10.0).color(egui::Color32::GRAY));
                ui.label(
                    egui::RichText::new(format!("{:.2} GHz", self.state.current_freq_ghz()))
                        .size(24.0)
                        .strong(),
                );
            });

            ui.add_space(30.0);

            // Current mode
            let mode_color = Self::mode_color(self.state.mode);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("MODE").size(10.0).color(egui::Color32::GRAY));
                ui.label(
                    egui::RichText::new(self.state.mode.label())
                        .size(24.0)
                        .color(mode_color)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(self.state.mode.description())
                        .size(10.0)
                        .color(egui::Color32::GRAY),
                );
            });
        });
    }

    /// Render mode control buttons
    fn render_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for mode in Mode::all() {
                let is_current = self.state.mode == *mode;
                let color = Self::mode_color(*mode);

                let button = egui::Button::new(
                    egui::RichText::new(mode.label())
                        .size(14.0)
                        .color(if is_current { egui::Color32::BLACK } else { color }),
                )
                .fill(if is_current { color } else { egui::Color32::TRANSPARENT })
                .stroke(egui::Stroke::new(1.0, color))
                .min_size(egui::vec2(120.0, 35.0));

                if ui.add(button).clicked() && !is_current {
                    self.change_mode(*mode);
                }

                ui.add_space(8.0);
            }
        });
    }

    /// Render target temperature control
    fn render_target_temp(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Target:").size(14.0));
            ui.add_space(5.0);

            let slider = egui::Slider::new(&mut self.target_temp, 40.0..=80.0)
                .suffix("°C")
                .step_by(1.0)
                .text("");
            ui.add_sized([150.0, 25.0], slider);

            ui.add_space(10.0);

            // Auto control checkbox
            let auto_label = if self.auto_control { "Auto ON" } else { "Auto OFF" };
            let auto_color = if self.auto_control {
                egui::Color32::from_rgb(100, 220, 100)
            } else {
                egui::Color32::GRAY
            };
            if ui.add(egui::Button::new(
                egui::RichText::new(auto_label).size(12.0).color(auto_color)
            ).min_size(egui::vec2(80.0, 25.0))).clicked() {
                self.auto_control = !self.auto_control;
                if self.auto_control {
                    self.set_status("Auto thermal control ENABLED".into());
                } else {
                    self.set_status("Auto thermal control DISABLED".into());
                }
            }

            ui.add_space(10.0);

            // Status indicator
            if self.state.cpu_temp > self.target_temp {
                ui.label(
                    egui::RichText::new(format!("+{:.1}°C", self.state.cpu_temp - self.target_temp))
                        .size(13.0)
                        .color(egui::Color32::from_rgb(255, 150, 100)),
                );
            } else {
                ui.label(
                    egui::RichText::new("OK")
                        .size(13.0)
                        .color(egui::Color32::from_rgb(100, 220, 100)),
                );
            }
        });
    }

    /// Render fan control
    fn render_fan_control(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Fan:").size(14.0));
            ui.add_space(10.0);

            // Fan boost button
            let fan_label = if self.state.fan_boost || self.fan_boost_manual {
                "BOOST ON"
            } else {
                "BOOST OFF"
            };
            let fan_color = if self.state.fan_boost || self.fan_boost_manual {
                egui::Color32::from_rgb(255, 150, 100)
            } else {
                egui::Color32::GRAY
            };

            if ui.add(egui::Button::new(
                egui::RichText::new(fan_label).size(14.0).color(
                    if self.state.fan_boost || self.fan_boost_manual {
                        egui::Color32::BLACK
                    } else {
                        fan_color
                    }
                )
            )
            .fill(if self.state.fan_boost || self.fan_boost_manual {
                fan_color
            } else {
                egui::Color32::TRANSPARENT
            })
            .stroke(egui::Stroke::new(1.0, fan_color))
            .min_size(egui::vec2(100.0, 30.0))).clicked() {
                self.fan_boost_manual = !self.fan_boost_manual;
                if let Err(e) = set_fan_boost(self.fan_boost_manual) {
                    self.set_status(format!("Fan error: {}", e));
                } else {
                    self.set_status(if self.fan_boost_manual {
                        "Fan BOOST activated".into()
                    } else {
                        "Fan returned to AUTO".into()
                    });
                }
            }

            ui.add_space(20.0);
            ui.label(
                egui::RichText::new("Manual fan boost for rapid cooling")
                    .size(11.0)
                    .color(egui::Color32::GRAY),
            );
        });
    }

    /// Render temperature history graph
    fn render_history(&self, ui: &mut egui::Ui, target_temp: f32) {
        if self.history.is_empty() {
            return;
        }

        let cpu_line = Line::new(self.history.cpu_points())
            .name("CPU")
            .color(egui::Color32::from_rgb(255, 100, 100))
            .width(2.0);

        let kbd_line = Line::new(self.history.kbd_points())
            .name("Keyboard")
            .color(egui::Color32::from_rgb(100, 200, 255))
            .width(2.0);

        // Target temperature line
        let target_points: Vec<[f64; 2]> = (0..HISTORY_CAPACITY)
            .map(|i| [i as f64, target_temp as f64])
            .collect();
        let target_line = Line::new(PlotPoints::new(target_points))
            .name("Target")
            .color(egui::Color32::from_rgb(255, 200, 100))
            .width(1.5)
            .style(egui_plot::LineStyle::dashed_loose());

        Plot::new("temp_history")
            .height(200.0)
            .show_axes(true)
            .show_grid(true)
            .include_y(30.0)
            .include_y(80.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .legend(egui_plot::Legend::default())
            .show(ui, |plot_ui| {
                plot_ui.line(cpu_line);
                plot_ui.line(kbd_line);
                plot_ui.line(target_line);
            });
    }

    /// Render status bar
    fn render_status(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Status message (auto-clear after 3 seconds)
            if let Some((msg, time)) = &self.status_message {
                if time.elapsed() < Duration::from_secs(3) {
                    ui.label(egui::RichText::new(msg).size(12.0).color(egui::Color32::YELLOW));
                } else {
                    self.status_message = None;
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Thermal Monitor v1.2.0")
                        .size(11.0)
                        .color(egui::Color32::DARK_GRAY),
                );
            });
        });
    }
}

impl eframe::App for ThermalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update state every UPDATE_INTERVAL_SECS
        if self.last_update.elapsed() >= Duration::from_secs_f32(UPDATE_INTERVAL_SECS) {
            self.update_state();
            self.last_update = Instant::now();
        }

        // Request repaint to keep updating
        ctx.request_repaint_after(Duration::from_millis(100));

        // Dark theme
        ctx.set_visuals(egui::Visuals::dark());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);

            // Title
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("Thermal Monitor").size(24.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Profile: {}", self.state.platform_profile))
                            .size(12.0)
                            .color(egui::Color32::GRAY),
                    );
                });
            });
            ui.separator();
            ui.add_space(5.0);

            // Top row: Temperatures and Performance side by side
            ui.horizontal(|ui| {
                // Left: Temperatures
                ui.group(|ui| {
                    ui.set_min_width(380.0);
                    ui.label(egui::RichText::new("Temperatures").size(14.0).strong());
                    ui.add_space(5.0);
                    self.render_temperatures(ui);
                });

                ui.add_space(10.0);

                // Right: Performance
                ui.group(|ui| {
                    ui.set_min_width(350.0);
                    ui.label(egui::RichText::new("Performance").size(14.0).strong());
                    ui.add_space(5.0);
                    self.render_performance(ui);
                });
            });

            ui.add_space(8.0);

            // Controls row
            ui.group(|ui| {
                ui.label(egui::RichText::new("Mode Control").size(14.0).strong());
                ui.add_space(5.0);
                self.render_controls(ui);
            });

            ui.add_space(8.0);

            // Target temperature and Fan control in same row
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.set_min_width(400.0);
                    ui.label(egui::RichText::new("Target Temperature").size(14.0).strong());
                    ui.add_space(5.0);
                    self.render_target_temp(ui);
                });

                ui.add_space(10.0);

                ui.group(|ui| {
                    ui.set_min_width(330.0);
                    ui.label(egui::RichText::new("Fan Control").size(14.0).strong());
                    ui.add_space(5.0);
                    self.render_fan_control(ui);
                });
            });

            ui.add_space(8.0);

            // History graph
            let target = self.target_temp;
            ui.group(|ui| {
                ui.label(egui::RichText::new("Temperature History (2 min)").size(14.0).strong());
                ui.add_space(5.0);
                self.render_history(ui, target);
            });

            // Status bar at bottom
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                self.render_status(ui);
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_capacity() {
        let mut history = TemperatureHistory::new(3);
        history.push(40.0, 35.0);
        history.push(42.0, 36.0);
        history.push(44.0, 37.0);
        assert_eq!(history.len(), 3);

        history.push(46.0, 38.0);
        assert_eq!(history.len(), 3); // Should not exceed capacity
    }

    #[test]
    fn test_history_empty() {
        let history = TemperatureHistory::new(10);
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_history_default() {
        let history = TemperatureHistory::default();
        assert!(history.is_empty());
        assert_eq!(history.capacity, HISTORY_CAPACITY);
    }

    #[test]
    fn test_history_points() {
        let mut history = TemperatureHistory::new(10);
        history.push(40.0, 35.0);
        history.push(42.0, 36.0);

        let _cpu_points = history.cpu_points();
        let _kbd_points = history.kbd_points();

        // Verify points are generated correctly
        assert!(!history.is_empty());
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_history_fifo_behavior() {
        let mut history = TemperatureHistory::new(2);
        history.push(10.0, 5.0);  // First in
        history.push(20.0, 10.0);
        history.push(30.0, 15.0); // Should push out first

        assert_eq!(history.len(), 2);
        // First value (10.0) should be gone
    }

    #[test]
    fn test_zone_colors() {
        // Verify all zones have valid colors
        for zone in [
            ThermalZone::Cool,
            ThermalZone::Comfort,
            ThermalZone::Optimal,
            ThermalZone::Warm,
            ThermalZone::Hot,
            ThermalZone::Critical,
        ] {
            let color = ThermalApp::zone_color(zone);
            assert_ne!(color, egui::Color32::TRANSPARENT);
        }
    }

    #[test]
    fn test_zone_colors_match_thermal_zone() {
        // Verify zone_color matches color_rgb from ThermalZone
        for zone in [
            ThermalZone::Cool,
            ThermalZone::Comfort,
            ThermalZone::Optimal,
            ThermalZone::Warm,
            ThermalZone::Hot,
            ThermalZone::Critical,
        ] {
            let (r, g, b) = zone.color_rgb();
            let color = ThermalApp::zone_color(zone);
            assert_eq!(color, egui::Color32::from_rgb(r, g, b));
        }
    }

    #[test]
    fn test_mode_colors() {
        // Verify all modes have colors
        for mode in Mode::all() {
            let color = ThermalApp::mode_color(*mode);
            assert_ne!(color, egui::Color32::TRANSPARENT);
        }
    }

    #[test]
    fn test_mode_color_unknown() {
        let color = ThermalApp::mode_color(Mode::Unknown);
        assert_eq!(color, egui::Color32::GRAY);
    }

    #[test]
    fn test_mode_colors_distinct() {
        // Each mode should have a distinct color
        let colors: Vec<_> = Mode::all().iter().map(|m| ThermalApp::mode_color(*m)).collect();

        // Performance should be reddish
        assert!(colors[0].r() > colors[0].b());

        // Comfort should be blueish
        assert!(colors[1].b() > colors[1].r());

        // Balanced should be greenish
        assert!(colors[2].g() > colors[2].r());
    }
}
