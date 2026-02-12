use atomic_float::AtomicF32;
use std::sync::atomic::Ordering;

use clack_plugin::events::event_types::ParamValueEvent;

pub const PARAM_GAIN_ID: u32 = 0;

pub struct Params {
    pub gain: AtomicF32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            gain: AtomicF32::new(1.0),
        }
    }
}

impl Params {
    pub fn gain(&self) -> f32 {
        self.gain.load(Ordering::Relaxed)
    }

    pub fn set_gain(&self, v: f32) {
        self.gain.store(v, Ordering::Relaxed);
    }

    pub fn handle_param_value_event(&self, event: &ParamValueEvent) {
        match event.param_id().map(|id| id.into()) {
            Some(PARAM_GAIN_ID) => self.set_gain(event.value() as f32),
            _ => {}
        }
    }
}
