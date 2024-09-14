use std::fmt::Display;

use circular_buffer::CircularBuffer;
use eframe::egui::{self, Event, Vec2};
use egui_plot::{Legend, Line, PlotPoints};

use nvml_wrapper::struct_wrappers::device::MemoryInfo;

#[derive(Debug, Clone, Copy)]
pub struct CudaDriverVersion {
    pub major: i32,
    pub minor: i32,
}

impl Display for CudaDriverVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[derive(Debug, Clone)]
pub struct DeviceState {
    pub name: String,
    pub driver_version: String,
    pub cuda_driver_version: CudaDriverVersion,
    pub temperature: u32,
    pub mem_info: MemoryInfo,
    pub fan_speeds: Vec<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct DeviceView {
    pub device_stats_plot: DeviceStatsPlot,
}

#[derive(Debug, Clone)]
pub struct DeviceStatsPlot {
    pub lock_x: bool,
    pub lock_y: bool,
    pub ctrl_to_zoom: bool,
    pub shift_to_horizontal: bool,
    pub zoom_speed: f32,
    pub scroll_speed: f32,
    pub temperature_vals: CircularBuffer<10_000, u32>,
    pub memory_usage_vals: CircularBuffer<10_000, u64>,
}

impl Default for DeviceStatsPlot {
    fn default() -> Self {
        Self {
            lock_x: false,
            lock_y: false,
            ctrl_to_zoom: false,
            shift_to_horizontal: false,
            zoom_speed: 1.0,
            scroll_speed: 1.0,
            temperature_vals: CircularBuffer::new(),
            memory_usage_vals: CircularBuffer::new(),
        }
    }
}

impl DeviceStatsPlot {
    pub fn plot_ui(&mut self, ui: &mut egui::Ui) {
        let (scroll, pointer_down, modifiers) = ui.input(|i| {
            let scroll = i.events.iter().find_map(|e| match e {
                Event::MouseWheel {
                    unit: _,
                    delta,
                    modifiers: _,
                } => Some(*delta),
                _ => None,
            });
            (scroll, i.pointer.primary_down(), i.modifiers)
        });

        egui_plot::Plot::new("plot")
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .legend(Legend::default())
            .show(ui, |plot_ui| {
                if let Some(mut scroll) = scroll {
                    if modifiers.ctrl == self.ctrl_to_zoom {
                        scroll = Vec2::splat(scroll.x + scroll.y);
                        let mut zoom_factor = Vec2::from([
                            (scroll.x * self.zoom_speed / 10.0).exp(),
                            (scroll.y * self.zoom_speed / 10.0).exp(),
                        ]);
                        if self.lock_x {
                            zoom_factor.x = 1.0;
                        }
                        if self.lock_y {
                            zoom_factor.y = 1.0;
                        }
                        plot_ui.zoom_bounds_around_hovered(zoom_factor);
                    } else {
                        if modifiers.shift == self.shift_to_horizontal {
                            scroll = Vec2::new(scroll.y, scroll.x);
                        }
                        if self.lock_x {
                            scroll.x = 0.0;
                        }
                        if self.lock_y {
                            scroll.y = 0.0;
                        }
                        let delta_pos = self.scroll_speed * scroll;
                        plot_ui.translate_bounds(delta_pos);
                    }
                }
                if plot_ui.response().hovered() && pointer_down {
                    let mut pointer_translate = -plot_ui.pointer_coordinate_drag_delta();
                    if self.lock_x {
                        pointer_translate.x = 0.0;
                    }
                    if self.lock_y {
                        pointer_translate.y = 0.0;
                    }
                    plot_ui.translate_bounds(pointer_translate);
                }

                // TODO(Thomas) This is wrong since the time is needed as well
                let temperature_points: PlotPoints = self
                    .temperature_vals
                    .iter()
                    .enumerate()
                    .map(|(i, &temp)| [i as f64, temp as f64])
                    .collect();

                let memory_usage_points: PlotPoints = self
                    .memory_usage_vals
                    .iter()
                    .enumerate()
                    .map(|(i, &mem_usage)| [i as f64, mem_usage as f64])
                    .collect();

                // Plot the temperature line
                plot_ui.line(Line::new(temperature_points).name("GPU Temperature"));
                plot_ui.line(Line::new(memory_usage_points).name("Memory Usage"));
            });
    }
}
