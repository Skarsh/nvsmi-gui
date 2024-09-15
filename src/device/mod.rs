use std::fmt::Display;

use circular_buffer::CircularBuffer;
use eframe::egui::{self, Color32};
use egui_plot::{Legend, Line, Plot, PlotPoints};

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
    pub power_usage: u32,
}

#[derive(Debug, Default, Clone)]
pub struct DeviceView {
    pub device_stats_plot: DeviceStatsPlot,
}

#[derive(Debug, Clone)]
pub struct DeviceStatsPlot {
    pub temperature_vals: CircularBuffer<5000, u32>,
    max_temperature: u32,
    pub memory_usage_vals: CircularBuffer<5000, u64>,
    max_memory_usage: u64,
    pub power_usage_vals: CircularBuffer<5000, u32>,
    max_power_usage: u32,
}

impl Default for DeviceStatsPlot {
    fn default() -> Self {
        Self {
            // TODO(Thomas): These max value asusmptions should come from a better place than this
            temperature_vals: CircularBuffer::new(),
            max_temperature: 100,
            memory_usage_vals: CircularBuffer::new(),
            max_memory_usage: 0,
            power_usage_vals: CircularBuffer::new(),
            max_power_usage: 1000,
        }
    }
}

impl DeviceStatsPlot {
    pub fn set_max_memory_usage(&mut self, max_memory_usage: u64) {
        self.max_memory_usage = max_memory_usage;
    }
}

impl DeviceStatsPlot {
    pub fn plot_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.set_height(400.0);
            egui_plot::Plot::new("temperature")
                .width(ui.available_width() / 2.0)
                .include_x(0)
                .include_y(0)
                .include_y(self.max_temperature)
                .allow_zoom(true)
                .allow_drag(true)
                .allow_scroll(false)
                .legend(Legend::default())
                .x_axis_label("measurements")
                .y_axis_label("deg")
                .show_grid(false)
                .show(ui, |plot_ui| {
                    let temperature_points: PlotPoints = self
                        .temperature_vals
                        .iter()
                        .enumerate()
                        .map(|(i, &temp)| [i as f64, temp as f64])
                        .collect();

                    plot_ui.line(
                        Line::new(temperature_points)
                            .name("GPU Temperature")
                            .color(Color32::from_rgb(168, 68, 13)),
                    );
                });

            Plot::new("memory usage")
                .width(ui.available_width())
                .include_x(0)
                .include_y(0)
                .include_y(self.max_memory_usage as f64)
                .allow_zoom(false)
                .allow_drag(false)
                .allow_scroll(false)
                .legend(Legend::default())
                .x_axis_label("measurements")
                .y_axis_label("MiB")
                .show_grid(false)
                .show(ui, |plot_ui| {
                    let memory_usage_points: PlotPoints = self
                        .memory_usage_vals
                        .iter()
                        .enumerate()
                        .map(|(i, &mem_usage)| [i as f64, mem_usage as f64])
                        .collect();

                    plot_ui.line(
                        Line::new(memory_usage_points)
                            .name("Memory Usage")
                            .color(Color32::from_rgb(95, 118, 156)),
                    );
                });
        });
        Plot::new("power usage")
            .width(ui.available_width() / 2.0)
            .include_x(0)
            .include_y(0)
            .include_y(self.max_power_usage as f64)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .legend(Legend::default())
            .x_axis_label("measurements")
            .y_axis_label("W")
            .show_grid(false)
            .show(ui, |plot_ui| {
                let power_usage_points: PlotPoints = self
                    .power_usage_vals
                    .iter()
                    .enumerate()
                    .map(|(i, &power_usage)| [i as f64, power_usage as f64])
                    .collect();

                plot_ui.line(
                    Line::new(power_usage_points)
                        .name("Power Usage")
                        .color(Color32::from_rgb(207, 184, 54)),
                );
            });
    }
}
