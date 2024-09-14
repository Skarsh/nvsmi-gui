use std::fmt::Display;

use circular_buffer::CircularBuffer;

use eframe::egui::{self, Color32, Event, Label, RichText, Vec2};
use egui_extras::{Column, TableBuilder};
use egui_plot::{Legend, Line, PlotPoints};

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::struct_wrappers::device::{MemoryInfo, ProcessInfo};
use nvml_wrapper::Nvml;

use once_cell::sync::Lazy;

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
    let process_state = ProcessState {
        processes
    };

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
    };

    SystemState {
        device_state,
        process_state
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
struct SystemState {
    device_state: DeviceState,
    process_state: ProcessState,
}

#[derive(Debug, Clone)]
struct DeviceState {
    name: String,
    driver_version: String,
    cuda_driver_version: CudaDriverVersion,
    temperature: u32,
    mem_info: MemoryInfo,
    fan_speeds: Vec<u32>,
}

#[derive(Debug, Default, Clone)]
struct DeviceView { 
    device_stats_plot: DeviceStatsPlot,
}

#[derive(Debug, Clone)]
struct ProcessState {
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

#[derive(Debug, Clone)]
struct ProcessTable {
    striped: bool,
    resizable: bool,
    clickable: bool,
    sort_descending: bool,
    sort_kind: Option<SortKind>,
    processes: Vec<ProcessData>,
    show_plot_window: bool,
    selection: std::collections::HashSet<usize>,
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
            show_plot_window: false,
            selection: Default::default(),
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

        let mut rows_to_toggle: Vec<(usize, egui::Response)> = Vec::new();

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
                        let row_index = row.index();
                        row.set_selected(self.selection.contains(&row_index));
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
                        let response = row.response();
                        if response.clicked() {
                            rows_to_toggle.push((row_index, response));
                        }
                    });
                }
            });

        // Toggle row selection after the table has been drawn
        for (row_index, response) in rows_to_toggle {
            self.toggle_row_selection(row_index, &response);
        }

        self.show_plot_window = !self.selection.is_empty();
    }

    fn toggle_row_selection(&mut self, row_index: usize, row_response: &egui::Response) {
        if row_response.clicked() {
            if self.selection.contains(&row_index) {
                self.selection.remove(&row_index);
            } else {
                self.selection.insert(row_index);
            }
        }
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
}

impl MyApp {
    fn new() -> Self {
        Self {
            current_state: None,
            device_view: DeviceView::default(),
            process_table: ProcessTable::default(),
            current_tab: Tab::Devices,
        }
    }
}

#[derive(Debug, Clone)]
struct DeviceStatsPlot {
    lock_x: bool,
    lock_y: bool,
    ctrl_to_zoom: bool,
    shift_to_horizontal: bool,
    zoom_speed: f32,
    scroll_speed: f32,
    temperature_vals: CircularBuffer<10_000, u32>,
    memory_usage_vals: CircularBuffer<10_000, u64>,
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
    fn plot_ui(&mut self, ui: &mut egui::Ui) {
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

        ui.label("This example shows how to use raw input events to implement different plot controls than the ones egui provides by default, e.g., default to zooming instead of panning when the Ctrl key is not pressed, or controlling much it zooms with each mouse wheel step.");

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

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for new values from the receiver
        let system_state = poll_device();
        self.current_state = Some(system_state.clone());
        self.process_table.processes = self.current_state.as_ref().unwrap().process_state.processes.clone();
        self.device_view.device_stats_plot.temperature_vals.push_back(system_state.device_state.temperature);
        self.device_view.device_stats_plot.memory_usage_vals.push_back(system_state.device_state.mem_info.used / 1_000_000);
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
            if let Some(system_state) = &mut self.current_state {
                match self.current_tab {
                    Tab::Devices => {
                        ui.heading("Device Information");
                        ui.add_space(10.0);

                        ui.label(format!("Device: {}", system_state.device_state.name));
                        ui.label(format!("Driver version: {}", system_state.device_state.driver_version));
                        ui.label(format!("CUDA version: {}", system_state.device_state.cuda_driver_version));

                        ui.add_space(10.0);

                        ui.label(format!("Temperature: {}°C", system_state.device_state.temperature));
                        ui.label(format!(
                            "Memory usage: {} MiB / {} MiB",
                            system_state.device_state.mem_info.used / 1_000_000,
                            system_state.device_state.mem_info.total / 1_000_000
                        ));

                        for (i, fan) in system_state.device_state.fan_speeds.iter().enumerate() {
                            ui.label(format!("Fan {} speed: {}%", i + 1, fan));
                        }

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
