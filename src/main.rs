use eframe::egui;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::high_level::Event::*;
use nvml_wrapper::struct_wrappers::device::MemoryInfo;
use nvml_wrapper::{
    error::{NvmlError, NvmlErrorWithSource},
    high_level::{EventLoop, EventLoopProvider},
    Device, Nvml,
};
use std::sync::mpsc::{self, channel, Receiver, Sender};

fn event_loop_thread(tx: Sender<DeviceState>) {
    let nvml = Nvml::init().unwrap();
    let device = nvml.device_by_index(0).unwrap();
    let mut event_loop = nvml.create_event_loop(vec![&device]).unwrap();

    event_loop.run_forever(|event, state| match event {
        // If there were no erors extract the event
        Ok(event) => match event {
            ClockChange(device) => {
                if let Ok(uuid) = device.uuid() {
                    println!("ClockChange event for device with UUID {:?}", uuid);
                    let device_state = DeviceState {
                        name: device.name().unwrap(),
                        temperature: device.temperature(TemperatureSensor::Gpu).unwrap(),
                        mem_info: device.memory_info().unwrap()
                    };
                    tx.send(device_state).unwrap();
                } else {
                    // Your error handling strategy here...
                }
            }
            PowerStateChange(device) => {
                if let Ok(uuid) = device.uuid() {
                    println!("PowerStateChange event for device with UUID {:?}", uuid);
                }
            }
            _ => println!("A different event occured: {:?}", event),
        },
        // If there was an error, handle it
        Err(e) => match e {
            // If the error is `Unknown`, continue looping and hope for the best.
            NvmlError::Unknown => {}

            // The other errors that can occur are almost guaranteed to mean that
            // further looping will never be successful (`GpuLost` and `Uninitilized`), so we stop
            // looping
            _ => state.interrupt(),
        },
    });
}


struct DeviceState {
    name: String,
    temperature: u32,
    mem_info: MemoryInfo,
}

fn main() -> eframe::Result {
    env_logger::init();

    let (tx, rx) = channel::<DeviceState>();

    std::thread::spawn(move || {
        event_loop_thread(tx);
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "nvmsi-gui",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(rx)))),
    )
}


struct MyApp {
    rx: Receiver<DeviceState>,
    current_state: Option<DeviceState>,
}

impl MyApp {
    fn new(rx: Receiver<DeviceState>) -> Self {
        Self {
            rx,
            current_state: None,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for new values from the receiver
        if let Ok(value) = self.rx.try_recv() {
            self.current_state = Some(value);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("nvsmi-gui");

            if let Some(device) = &self.current_state {
                ui.label(format!("name: {}", device.name));
                ui.label(format!("temperature: {}°C", device.temperature));
                ui.label(format!("memory free: {}", device.mem_info.free));
                ui.label(format!("memory total: {}", device.mem_info.total));
                ui.label(format!("memory used: {}", device.mem_info.used));
            } else {
                ui.label("Waiting for data...");
            }
        });

        // Request a repaint on the next frame
        ctx.request_repaint();
    }
}
