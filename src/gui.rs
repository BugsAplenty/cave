use std::sync::Arc;
use std::sync::atomic::Ordering;

use atomic_float::AtomicF32;
use baseview::{Size, WindowHandle, WindowOpenOptions, WindowScalePolicy};
use clack_plugin::plugin::PluginError;
use egui_baseview::{EguiWindow, GraphicsConfig, Queue};
use egui_baseview::egui::{self, Context, Slider};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

use crate::params::Params as CaveParams;

pub struct CaveGui {
    pub parent: Option<RawWindowHandle>,
    handle: Option<WindowHandle>,
}

impl Default for CaveGui {
    fn default() -> Self {
        Self {
            parent: None,
            handle: None,
        }
    }
}

impl CaveGui {
    pub fn is_open(&self) -> bool {
        self.handle.is_some()
    }
    pub fn open(&mut self, params: Arc<CaveParams>) -> Result<(), PluginError> {
        eprintln!("[cave-gui] open() called");

        let Some(parent) = self.parent else {
            eprintln!("[cave-gui] ERROR: parent is None (set_parent() likely never ran)");
            return Err(PluginError::Message("No parent window provided"));
        };

        eprintln!("[cave-gui] parent handle = {:?}", parent);

        // (Optional but helpful) refuse handle types we know won't work for embedded windows
        // so Bitwig gets an explicit error instead of timing out.
        #[cfg(target_os = "linux")]
        {
            match parent {
                RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_) => {
                    eprintln!("[cave-gui] Linux: got X11 handle (good for open_parented)");
                }
                RawWindowHandle::Wayland(_) => {
                    eprintln!("[cave-gui] Linux: got WAYLAND handle (embedded UI usually won't work)");
                    // IMPORTANT: If Bitwig expects embedded, returning Err here will still mean “no GUI”,
                    // but it makes the failure explicit and prevents false-success.
                    return Err(PluginError::Message(
                        "Got Wayland parent handle; embedded editor not supported in this build",
                    ));
                }
                other => {
                    eprintln!("[cave-gui] Linux: unsupported parent handle: {:?}", other);
                    return Err(PluginError::Message(
                        "Unsupported parent window handle type",
                    ));
                }
            }
        }

        let settings = WindowOpenOptions {
            title: "Cave".to_string(),
            size: Size::new(400.0, 300.0),
            scale: WindowScalePolicy::SystemScaleFactor,
            gl_config: Some(Default::default()),
        };

        eprintln!("[cave-gui] calling EguiWindow::open_parented(...)");

        // If this returns but Bitwig still says “did not create its window”, then either:
        // - baseview failed internally without panicking,
        // - or the parent handle doesn't match what baseview expects at runtime.
        self.handle = Some(EguiWindow::open_parented(
            self,
            settings,
            GraphicsConfig::default(),
            params,
            |_egui_ctx: &Context, _queue: &mut Queue, _state: &mut Arc<CaveParams>| {},
            |egui_ctx: &Context, _queue: &mut Queue, state: &mut Arc<CaveParams>| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    ui.heading("Cave Synth");
                    Self::slider(ui, &state.gain, "Gain");
                });
            },
        ));

        eprintln!("[cave-gui] open_parented returned, handle is set");
        Ok(())
    }

    pub fn close(&mut self) {
        eprintln!("[cave-gui] close() called");
        if let Some(handle) = self.handle.as_mut() {
            handle.close();
        }
        self.handle = None;
    }

    fn slider(ui: &mut egui::Ui, property: &AtomicF32, name: &str) {
        let mut value = property.load(Ordering::Relaxed);
        if ui.add(Slider::new(&mut value, 0.0..=1.0).text(name)).changed() {
            property.store(value, Ordering::Relaxed);
        }
    }
}

unsafe impl HasRawWindowHandle for CaveGui {
    fn raw_window_handle(&self) -> RawWindowHandle {
        // If Bitwig never called set_parent(), this will panic (useful: you'll see it in logs).
        self.parent.expect("Parent window not set")
    }
}
