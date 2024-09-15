use std::time::{Duration, Instant};

use eframe::egui;

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;

use once_cell::sync::Lazy;

mod device;
use device::{CudaDriverVersion, DeviceState, DeviceView};

mod process;
use process::{ProcessData, ProcessKind, ProcessState, ProcessTable};

static NVML: Lazy<Nvml> = Lazy::new(|| Nvml::init().unwrap());

fn poll_device() -> SystemState {
    let device = NVML.device_by_index(0).unwrap();
    let cuda_driver_version = NVML.sys_cuda_driver_version().unwrap();
    let running_graphics_processes = device.running_graphics_processes().unwrap();

    let graphics_process_names: Vec<String> = running_graphics_processes
        .iter()
        .map(|process| {
            NVML.sys_process_name(process.pid, 64)
                .unwrap_or_else(|_| String::from("Unknown"))
        })
        .collect();

    let graphics_process_data_vec: Vec<ProcessData> = running_graphics_processes
        .iter()
        .zip(graphics_process_names)
        .map(|(process_info, process_name)| ProcessData {
            process_info: process_info.clone(),
            process_kind: ProcessKind::Graphics,
            process_name: process::get_process_name(&process_name)
                .to_string()
                .to_lowercase(),
        })
        .collect();

    let running_compute_processes = device.running_compute_processes().unwrap();
    let compute_process_names: Vec<String> = running_compute_processes
        .iter()
        .map(|process| {
            NVML.sys_process_name(process.pid, 64)
                .unwrap_or_else(|_| String::from("Unknown"))
        })
        .collect();

    let compute_process_data_vec: Vec<ProcessData> = running_compute_processes
        .iter()
        .zip(compute_process_names)
        .map(|(process_info, process_name)| ProcessData {
            process_info: process_info.clone(),
            process_kind: ProcessKind::Compute,
            process_name: process::get_process_name(&process_name).to_string(),
        })
        .collect();

    let processes = [graphics_process_data_vec, compute_process_data_vec].concat();
    let process_state = ProcessState { processes };

    let num_fans = device.num_fans().unwrap();
    let mut fan_speeds = Vec::new();
    for fan_idx in 0..num_fans {
        fan_speeds.push(device.fan_speed(fan_idx).unwrap());
    }

    let device_state = DeviceState {
        name: device.name().unwrap(),
        driver_version: NVML.sys_driver_version().unwrap(),
        cuda_driver_version: CudaDriverVersion {
            major: nvml_wrapper::cuda_driver_version_major(cuda_driver_version),
            minor: nvml_wrapper::cuda_driver_version_minor(cuda_driver_version),
        },
        temperature: device.temperature(TemperatureSensor::Gpu).unwrap(),
        mem_info: device.memory_info().unwrap(),
        fan_speeds,
        power_usage: device.power_usage().unwrap(),
    };

    SystemState {
        device_state,
        process_state,
    }
}

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "nvsmi-gui",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::new()))),
    )
    .unwrap();

    Ok(())
}

#[derive(Debug, Clone)]
struct SystemState {
    device_state: DeviceState,
    process_state: ProcessState,
}

#[derive(Debug, Clone)]
enum Tab {
    Devices,
    Processes,
}

struct MyApp {
    current_state: Option<SystemState>,
    device_view: DeviceView,
    process_table: ProcessTable,
    current_tab: Tab,
    last_update: Instant,
    update_interval: Duration,
}

impl MyApp {
    fn new() -> Self {
        let current_state = poll_device();
        let mut device_view = DeviceView::default();
        device_view
            .device_stats_plot
            .set_max_memory_usage(current_state.device_state.mem_info.total / 1_000_000);
        Self {
            current_state: Some(current_state),
            device_view,
            process_table: ProcessTable::default(),
            current_tab: Tab::Devices,
            last_update: Instant::now(),
            update_interval: Duration::from_millis(20),
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let system_state = poll_device();
        self.current_state = Some(system_state.clone());
        self.process_table.processes = self
            .current_state
            .as_ref()
            .unwrap()
            .process_state
            .processes
            .clone();
        self.process_table.sort_processes();

        if now.duration_since(self.last_update) >= self.update_interval {
            self.device_view
                .device_stats_plot
                .temperature_vals
                .push_back(system_state.device_state.temperature);
            self.device_view
                .device_stats_plot
                .memory_usage_vals
                .push_back(system_state.device_state.mem_info.used / 1_000_000);
            self.device_view
                .device_stats_plot
                .power_usage_vals
                .push_back(system_state.device_state.power_usage / 1000);
            self.last_update = now;
        }

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(matches!(self.current_tab, Tab::Devices), "Device(s)")
                    .clicked()
                {
                    self.current_tab = Tab::Devices;
                }
                if ui
                    .selectable_label(matches!(self.current_tab, Tab::Processes), "Processes")
                    .clicked()
                {
                    self.current_tab = Tab::Processes;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(system_state) = &mut self.current_state {
                match self.current_tab {
                    Tab::Devices => {
                        ui.heading("Device Information");
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.label(format!("Device: {}", system_state.device_state.name));
                            ui.label(format!(
                                "Driver version: {}",
                                system_state.device_state.driver_version
                            ));
                            ui.label(format!(
                                "CUDA version: {}",
                                system_state.device_state.cuda_driver_version
                            ));
                        });
                        ui.add_space(10.0);

                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "Temperature: {}°C",
                                system_state.device_state.temperature
                            ));
                            ui.label(format!(
                                "Memory usage: {} MiB / {} MiB",
                                system_state.device_state.mem_info.used / 1_000_000,
                                system_state.device_state.mem_info.total / 1_000_000
                            ));
                        });

                        ui.horizontal(|ui| {
                            for (i, fan) in system_state.device_state.fan_speeds.iter().enumerate()
                            {
                                ui.label(format!("Fan {} speed: {}%", i + 1, fan));
                            }
                        });

                        ui.label(format!(
                            "Power usage: {}W",
                            system_state.device_state.power_usage / 1000
                        ));

                        ui.add_space(10.0);

                        self.device_view.device_stats_plot.plot_ui(ui);
                    }
                    Tab::Processes => {
                        ui.heading("Process Information");
                        ui.add_space(10.0);

                        self.process_table.table_ui(ui);

                        if self.process_table.show_plot_window {}
                    }
                }
            } else {
                ui.label("Waiting for data...");
            }
        });

        // Request a repaint on the next frame
        ctx.request_repaint();

        // Do potential cleanup stuff here
        if ctx.input(|i| i.viewport().close_requested()) {}
    }
}
