use std::fmt::Display;

use eframe::egui::{self, Color32, Label, RichText};
use egui_extras::{Column, TableBuilder};

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::struct_wrappers::device::{MemoryInfo, ProcessInfo};
use nvml_wrapper::Nvml;

use once_cell::sync::Lazy;

static NVML: Lazy<Nvml> = Lazy::new(|| Nvml::init().unwrap());

fn poll_device() -> DeviceState {
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
            process_name,
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
            process_name,
        })
        .collect();

    let processes = [graphics_process_data_vec, compute_process_data_vec].concat();

    let num_fans = device.num_fans().unwrap();
    let mut fan_speeds = Vec::new();
    for fan_idx in 0..num_fans {
        fan_speeds.push(device.fan_speed(fan_idx).unwrap());
    }

    DeviceState {
        name: device.name().unwrap(),
        driver_version: NVML.sys_driver_version().unwrap(),
        cuda_driver_version: CudaDriverVersion {
            major: nvml_wrapper::cuda_driver_version_major(cuda_driver_version),
            minor: nvml_wrapper::cuda_driver_version_minor(cuda_driver_version),
        },
        temperature: device.temperature(TemperatureSensor::Gpu).unwrap(),
        mem_info: device.memory_info().unwrap(),
        fan_speeds,
        processes,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ProcessKind {
    Compute,
    Graphics,
}

impl Display for ProcessKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessKind::Compute => write!(f, "Compute"),
            ProcessKind::Graphics => write!(f, "Graphics"),
        }
    }
}

#[derive(Debug, Clone)]
struct ProcessData {
    process_info: ProcessInfo,
    process_kind: ProcessKind,
    process_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKind {
    Pid,
    Type,
    ProcessName,
    Memory,
}

struct ProcessTable {
    striped: bool,
    resizable: bool,
    clickable: bool,
    sort_descending: bool,
    sort_kind: Option<SortKind>,
    processes: Vec<ProcessData>,
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self {
            striped: true,
            resizable: true,
            clickable: true,
            sort_descending: true,
            sort_kind: None,
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
            table = table.sense(egui::Sense::hover());
        }

        table
            .header(20.0, |mut header| {
                self.create_sortable_header(&mut header, "PID", SortKind::Pid);
                self.create_sortable_header(&mut header, "Type", SortKind::Type);
                self.create_sortable_header(&mut header, "Process name", SortKind::ProcessName);
                self.create_sortable_header(&mut header, "GPU Memory Usage", SortKind::Memory);
            })
            .body(|mut body| {
                for process in &self.processes {
                    let row_height = 30.0;
                    body.row(row_height, |mut row| {
                        row.col(|ui| {
                            ui.label(process.process_info.pid.to_string());
                        });
                        row.col(|ui| {
                            ui.label(process.process_kind.to_string());
                        });
                        row.col(|ui| {
                            ui.label(&process.process_name);
                        });
                        row.col(|ui| {
                            let mem_str = match process.process_info.used_gpu_memory {
                                UsedGpuMemory::Used(val) => format!("{} MiB", (val / 1_000_000)),
                                UsedGpuMemory::Unavailable => String::from("Unavailable"),
                            };
                            ui.label(mem_str);
                        });
                    });
                }
            });
    }

    fn create_sortable_header(
        &mut self,
        header: &mut egui_extras::TableRow,
        label: &str,
        sort_kind: SortKind,
    ) {
        header.col(|ui| {
            let rich_text = RichText::new(label).color(Color32::WHITE);
            let label = Label::new(rich_text);
            let response = ui.add(label.sense(egui::Sense::hover()));

            if response.hovered() {
                response.clone().highlight();
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }

            if response.clicked() {
                match self.sort_kind {
                    Some(current_sort) if current_sort == sort_kind => {
                        // If clicking on the same column, toggle the sort order
                        self.sort_descending = !self.sort_descending;
                    }
                    _ => {
                        // If clicking on a new column or sorting for the first time,
                        // set to descending order
                        self.sort_kind = Some(sort_kind);
                        self.sort_descending = true;
                    }
                }
                self.sort_processes();
            }
        });
    }

    fn sort_processes(&mut self) {
        if let Some(sort_kind) = self.sort_kind {
            self.processes.sort_by(|a, b| {
                let cmp = match sort_kind {
                    SortKind::Pid => a.process_info.pid.cmp(&b.process_info.pid),
                    SortKind::Type => a.process_kind.cmp(&b.process_kind),
                    SortKind::ProcessName => a.process_name.cmp(&b.process_name),
                    SortKind::Memory => {
                        let memory_a = match a.process_info.used_gpu_memory {
                            UsedGpuMemory::Used(val) => val,
                            UsedGpuMemory::Unavailable => 0,
                        };
                        let memory_b = match b.process_info.used_gpu_memory {
                            UsedGpuMemory::Used(val) => val,
                            UsedGpuMemory::Unavailable => 0,
                        };
                        memory_a.cmp(&memory_b)
                    }
                };
                if self.sort_descending {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
    }
}

enum Tab {
    Devices,
    Processes,
}

struct MyApp {
    current_state: Option<DeviceState>,
    process_table: ProcessTable,
    current_tab: Tab,
}

impl MyApp {
    fn new() -> Self {
        Self {
            current_state: None,
            process_table: ProcessTable::default(),
            current_tab: Tab::Devices,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for new values from the receiver
        let device_state = poll_device();
        self.current_state = Some(device_state);
        self.process_table.processes = self.current_state.as_ref().unwrap().processes.clone();
        self.process_table.sort_processes();

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
            if let Some(device) = &mut self.current_state {
                match self.current_tab {
                    Tab::Devices => {
                        ui.heading("Device Information");
                        ui.add_space(10.0);

                        ui.label(format!("Device: {}", device.name));
                        ui.label(format!("Driver version: {}", device.driver_version));
                        ui.label(format!("CUDA version: {}", device.cuda_driver_version));

                        ui.add_space(10.0);

                        ui.label(format!("Temperature: {}°C", device.temperature));
                        ui.label(format!(
                            "Memory usage: {} MiB / {} MiB",
                            device.mem_info.used / 1_000_000,
                            device.mem_info.total / 1_000_000
                        ));

                        for (i, fan) in device.fan_speeds.iter().enumerate() {
                            ui.label(format!("Fan {} speed: {}%", i + 1, fan));
                        }

                        // Add more device-specific information here
                    }
                    Tab::Processes => {
                        ui.heading("Process Information");
                        ui.add_space(10.0);
                        self.process_table.table_ui(ui);
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
