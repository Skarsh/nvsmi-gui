use std::fmt::Display;
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::struct_wrappers::device::{MemoryInfo, ProcessInfo};
use nvml_wrapper::Nvml;

// TODO(Thomas): Graceful shutdown
fn device_polling_thread(
    tx: Sender<DeviceState>,
    app_rx: Receiver<AppCommand>,
    poll_interval: Duration,
) {
    let nvml = Nvml::init().unwrap();
    let device = nvml.device_by_index(0).unwrap();

    let mut next_time = Instant::now() + poll_interval;

    loop {
        let now = Instant::now();
        if app_rx.try_recv().is_ok() {
            break;
        };
        if now >= next_time {
            // Query device
            let cuda_driver_version = nvml.sys_cuda_driver_version().unwrap();
            let running_graphics_processes = device.running_graphics_processes().unwrap();

            let graphics_process_names: Vec<String> = running_graphics_processes
                .iter()
                .map(|process| {
                    nvml.sys_process_name(process.pid, 64)
                        .unwrap_or_else(|_| String::from("Unknown"))
                })
                .collect();

            let graphics_process_data_vec: Vec<ProcessData> = running_graphics_processes
                .iter()
                .zip(graphics_process_names)
                .map(|(process_info, process_name)| ProcessData {
                    process_info: process_info.clone(),
                    process_kind: ProcessKind::Graphics,
                    process_name,
                })
                .collect();

            let running_compute_processes = device.running_compute_processes().unwrap();
            let compute_process_names: Vec<String> = running_compute_processes
                .iter()
                .map(|process| {
                    nvml.sys_process_name(process.pid, 64)
                        .unwrap_or_else(|_| String::from("Unknown"))
                })
                .collect();

            let compute_process_data_vec: Vec<ProcessData> = running_compute_processes
                .iter()
                .zip(compute_process_names)
                .map(|(process_info, process_name)| ProcessData {
                    process_info: process_info.clone(),
                    process_kind: ProcessKind::Compute,
                    process_name,
                })
                .collect();

            let processes = [graphics_process_data_vec, compute_process_data_vec].concat();

            let num_fans = device.num_fans().unwrap();
            let mut fan_speeds = Vec::new();
            for fan_idx in 0..num_fans {
                fan_speeds.push(device.fan_speed(fan_idx).unwrap());
            }

            let device_state = DeviceState {
                name: device.name().unwrap(),
                driver_version: nvml.sys_driver_version().unwrap(),
                cuda_driver_version: CudaDriverVersion {
                    major: nvml_wrapper::cuda_driver_version_major(cuda_driver_version),
                    minor: nvml_wrapper::cuda_driver_version_minor(cuda_driver_version),
                },
                temperature: device.temperature(TemperatureSensor::Gpu).unwrap(),
                mem_info: device.memory_info().unwrap(),
                fan_speeds,
                processes,
            };

            tx.send(device_state).unwrap();

            next_time += poll_interval;
        }

        // Calculate how long to sleep
        let sleep_duration = next_time - now;
        thread::sleep(sleep_duration);
    }
}

enum AppCommand {
    Exit,
}

fn main() -> eframe::Result {
    env_logger::init();

    let (tx, rx) = channel::<DeviceState>();
    let (app_tx, app_rx) = sync_channel::<AppCommand>(1);

    let device_polling_thread_handle = std::thread::spawn(move || {
        device_polling_thread(tx, app_rx, Duration::from_millis(100));
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "nvsmi-gui",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::new(app_tx, rx)))),
    )
    .unwrap();

    device_polling_thread_handle.join().unwrap();

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct CudaDriverVersion {
    major: i32,
    minor: i32,
}

impl Display for CudaDriverVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[derive(Debug, Clone)]
struct DeviceState {
    name: String,
    driver_version: String,
    cuda_driver_version: CudaDriverVersion,
    temperature: u32,
    mem_info: MemoryInfo,
    fan_speeds: Vec<u32>,
    processes: Vec<ProcessData>,
}

struct MyApp {
    app_tx: SyncSender<AppCommand>,
    rx: Receiver<DeviceState>,
    current_state: Option<DeviceState>,
    process_table: ProcessTable,
}

impl MyApp {
    fn new(app_tx: SyncSender<AppCommand>, rx: Receiver<DeviceState>) -> Self {
        Self {
            app_tx,
            rx,
            current_state: None,
            process_table: ProcessTable::default(),
        }
    }
}

#[derive(Debug, Clone)]
enum ProcessKind {
    Compute,
    Graphics,
}

#[derive(Debug, Clone)]
struct ProcessData {
    process_info: ProcessInfo,
    process_kind: ProcessKind,
    process_name: String,
}

#[derive(Debug)]
struct ProcessTable {
    striped: bool,
    resizable: bool,
    clickable: bool,
    process_memory_sorted_descending: bool,
    processes: Vec<ProcessData>,
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self {
            striped: true,
            resizable: true,
            clickable: true,
            process_memory_sorted_descending: false,
            processes: Vec::new(),
        }
    }
}

impl ProcessTable {
    fn table_ui(&mut self, ui: &mut egui::Ui) {
        let mut table = TableBuilder::new(ui)
            .striped(self.striped)
            .resizable(self.resizable)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::remainder())
            .column(Column::remainder());

        if self.clickable {
            table = table.sense(egui::Sense::click());
        }

        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("PID");
                });
                header.col(|ui| {
                    ui.strong("Type");
                });
                header.col(|ui| {
                    ui.strong("Process name");
                });
                header.col(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong("GPU Memory Usage");
                        if ui
                            .button(if self.process_memory_sorted_descending {
                                "⬆"
                            } else {
                                "⬇"
                            })
                            .clicked()
                        {
                            self.process_memory_sorted_descending =
                                !self.process_memory_sorted_descending
                        }
                    });
                });
            })
            .body(|mut body| {
                for row_index in 0..self.processes.len() {
                    let row_height = 30.0;
                    body.row(row_height, |mut row| {
                        row.col(|ui| {
                            ui.label(
                                self.processes
                                    .get(row_index)
                                    .unwrap()
                                    .process_info
                                    .pid
                                    .to_string(),
                            );
                        });

                        row.col(|ui| {
                            match &self.processes.get(row_index).unwrap().process_kind {
                                ProcessKind::Compute => ui.label("Compute"),
                                ProcessKind::Graphics => ui.label("Graphics"),
                            };
                        });

                        row.col(|ui| {
                            ui.label(&self.processes.get(row_index).unwrap().process_name);
                        });

                        row.col(|ui| {
                            let mem_str = match self
                                .processes
                                .get(row_index)
                                .unwrap()
                                .process_info
                                .used_gpu_memory
                            {
                                UsedGpuMemory::Used(val) => &format!("{} MiB", (val / 1_000_000)),
                                UsedGpuMemory::Unavailable => "Unavailable",
                            };

                            ui.label(mem_str);
                        });
                    })
                }
            });
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for new values from the receiver
        if let Ok(value) = self.rx.try_recv() {
            self.current_state = Some(value);
            self.process_table.processes = self.current_state.as_ref().unwrap().processes.clone();

            // Sort processes by memory usage if process_memory_sorted is true
            if self.process_table.process_memory_sorted_descending {
                self.process_table.processes.sort_by(|a, b| {
                    let memory_a = match a.process_info.used_gpu_memory {
                        UsedGpuMemory::Used(val) => val,
                        UsedGpuMemory::Unavailable => 0,
                    };
                    let memory_b = match b.process_info.used_gpu_memory {
                        UsedGpuMemory::Used(val) => val,
                        UsedGpuMemory::Unavailable => 0,
                    };

                    memory_b.cmp(&memory_a)
                })
            } else {
                self.process_table.processes.sort_by(|a, b| {
                    let memory_a = match a.process_info.used_gpu_memory {
                        UsedGpuMemory::Used(val) => val,
                        UsedGpuMemory::Unavailable => 0,
                    };
                    let memory_b = match b.process_info.used_gpu_memory {
                        UsedGpuMemory::Used(val) => val,
                        UsedGpuMemory::Unavailable => 0,
                    };

                    memory_a.cmp(&memory_b)
                })
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(device) = &mut self.current_state {
                ui.horizontal(|ui| {
                    ui.label(format!("Device: {}", device.name));
                    ui.label(format!("Driver version: {}", device.driver_version));
                    ui.label(format!("CUDA version: {}", device.cuda_driver_version));
                });

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.strong(format!("Temp: {}°C", device.temperature));
                    ui.strong(format!(
                        "Memory usage {} MiB / {} MiB",
                        device.mem_info.used / 1_000_000,
                        device.mem_info.total / 1_000_000
                    ));
                    for fan in &device.fan_speeds {
                        ui.strong(format!("Fan speed: {}%", fan));
                    }
                });

                ui.add_space(10.0);

                self.process_table.table_ui(ui);
            } else {
                ui.label("Waiting for data...");
            }
        });

        // Request a repaint on the next frame
        ctx.request_repaint();

        if ctx.input(|i| i.viewport().close_requested()) {
            self.app_tx.send(AppCommand::Exit).unwrap();
        }
    }
}
