use std::collections::HashSet;
use std::fmt::Display;

use eframe::egui::{self, Color32, Label, RichText};
use egui_extras::{Column, TableBuilder};

use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::struct_wrappers::device::ProcessInfo;

#[derive(Debug, Clone)]
pub struct ProcessState {
    pub processes: Vec<ProcessData>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessKind {
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
pub struct ProcessData {
    pub process_info: ProcessInfo,
    pub process_kind: ProcessKind,
    pub process_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKind {
    Pid,
    Type,
    ProcessName,
    Memory,
}

#[derive(Debug, Clone)]
pub struct ProcessTable {
    striped: bool,
    resizable: bool,
    clickable: bool,
    sort_descending: bool,
    sort_kind: Option<SortKind>,
    pub processes: Vec<ProcessData>,
    pub show_plot_window: bool,
    selection: HashSet<usize>,
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
    pub fn table_ui(&mut self, ui: &mut egui::Ui) {
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

    pub fn sort_processes(&mut self) {
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
