mod gui;
mod params;

use std::ffi::CStr;
use std::sync::Arc;

use clack_plugin::events::spaces::CoreEventSpace;
use clack_plugin::prelude::*;
use clack_plugin::{
    clack_export_entry,
    entry::{DefaultPluginFactory, SinglePluginEntry},
    host::{HostAudioProcessorHandle, HostMainThreadHandle, HostSharedHandle},
    plugin::{
        Plugin, PluginAudioProcessor, PluginDescriptor, PluginError, PluginMainThread, PluginShared,
    },
    process::{Audio, Events, PluginAudioConfiguration, Process, ProcessStatus},
};

// Extension imports
use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
    PluginAudioPortsImpl,
};
use clack_extensions::note_ports::{
    PluginNotePorts, NotePortInfo, NotePortInfoWriter, PluginNotePortsImpl, NoteDialect
};
use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, PluginGui, PluginGuiImpl, Window};
use clack_extensions::params::{
    ParamDisplayWriter, ParamInfo, ParamInfoFlags, ParamInfoWriter, PluginAudioProcessorParams,
    PluginMainThreadParams, PluginParams,
};

use raw_window_handle::HasRawWindowHandle;

use crate::gui::CaveGui;
use crate::params::{Params as CaveParams, PARAM_GAIN_ID};

pub struct Cave;

pub struct CaveShared {
    params: Arc<CaveParams>,
}

impl Default for CaveShared {
    fn default() -> Self {
        Self {
            params: Arc::new(CaveParams::default()),
        }
    }
}

impl<'a> PluginShared<'a> for CaveShared {}

pub struct CaveMainThread<'a> {
    shared: &'a CaveShared,
    gui: CaveGui,
}

impl<'a> PluginMainThread<'a, CaveShared> for CaveMainThread<'a> {}

pub struct CaveAudioProcessor<'a> {
    shared: &'a CaveShared,
    phase: f32,       // 0.0 to 1.0
    frequency: f32,   // Hz
    sample_rate: f32, // Hz
    note_on: bool,    // Is key pressed?
}

impl<'a> PluginAudioProcessor<'a, CaveShared, CaveMainThread<'a>> for CaveAudioProcessor<'a> {
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &mut CaveMainThread<'a>,
        shared: &'a CaveShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self {
            shared,
            phase: 0.0,
            frequency: 440.0,
            sample_rate: audio_config.sample_rate as f32,
            note_on: false,
        })
    }

        fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        // ... (Event handling same as above) ...
        // Copy the event handling code from above block
        for batch in events.input.batch() {
            for event in batch.events() {
                if let Some(event) = event.as_core_event() {
                    use clack_plugin::events::spaces::CoreEventSpace::*;
                    match event {
                        NoteOn(e) => {
                            if let clack_plugin::events::Match::Specific(key) = e.key() {
                                self.frequency = midi_to_freq(key as u8);
                                self.note_on = true;
                            }
                        }
                        NoteOff(e) => {
                            if let clack_plugin::events::Match::Specific(_) = e.key() {
                                self.note_on = false;
                            }
                        }
                        ParamValue(e) => self.shared.params.handle_param_value_event(e),
                        _ => {}
                    }
                }
            }
        }

        let gain = self.shared.params.gain();
        let phase_step = self.frequency / self.sample_rate;

        for mut port_pair in &mut audio {
            let Some(mut channels) = port_pair.channels()?.into_f32() else { continue };
            
            // Get the raw sample count
            let frame_count = port_pair.frames_count();
            
            // We'll generate the synth output into a temporary buffer (scratch space)
            // so we can copy it to both Left and Right channels identically.
            // (Allocating a vec in audio thread is bad practice, but for 1024 floats it's "okay" for a toy.
            //  Real plugins use a pre-allocated buffer in the struct).
            let mut synth_buffer = vec![0.0; frame_count as usize];
            
            // Generate Audio into temp buffer
            for sample in synth_buffer.iter_mut() {
                if self.note_on {
                    self.phase += phase_step;
                    if self.phase > 1.0 { self.phase -= 1.0; }
                    let raw = if self.phase < 0.5 { 1.0 } else { -1.0 };
                    *sample = raw * gain * 0.1;
                } else {
                    *sample = 0.0;
                }
            }

            // Copy temp buffer to all output channels
            for channel_pair in channels.iter_mut() {
                if let ChannelPair::OutputOnly(out_buf) = channel_pair {
                    // Optimized copy
                    out_buf.copy_from_slice(&synth_buffer);
                }
            }
        }

        Ok(ProcessStatus::Continue)
    }
}

impl Plugin for Cave {
    type AudioProcessor<'a> = CaveAudioProcessor<'a>;
    type Shared<'a> = CaveShared;
    type MainThread<'a> = CaveMainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Self::Shared<'_>>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginParams>()
            .register::<PluginGui>()
            .register::<PluginNotePorts>();
    }
}

impl<'a> PluginNotePortsImpl for CaveMainThread<'a> {
    fn count(&mut self, is_input: bool) -> u32 {
        if is_input { 1 } else { 0 }
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut NotePortInfoWriter) {
        if !is_input || index != 0 { return; }

        writer.set(&NotePortInfo {
            id: ClapId::new(0),
            name: b"MIDI Input",
            preferred_dialect: Some(NoteDialect::Clap),
            supported_dialects: NoteDialect::Clap.into(),
        });
    }
}

impl DefaultPluginFactory for Cave {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::*;
        PluginDescriptor::new("com.razboy.cave", "Cave")
            .with_vendor("razboy")
            .with_features([INSTRUMENT, SYNTHESIZER, STEREO])
    }

    fn new_shared(_host: HostSharedHandle) -> Result<Self::Shared<'_>, PluginError> {
        Ok(CaveShared::default())
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        Ok(CaveMainThread {
            shared,
            gui: CaveGui::default(),
        })
    }
}

impl<'a> PluginAudioPortsImpl for CaveMainThread<'a> {
    fn count(&mut self, is_input: bool) -> u32 {
        if is_input { 0 } else { 1 }
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if is_input || index != 0 { return; }

        writer.set(&AudioPortInfo {
            id: ClapId::new(0),
            name: b"Output",
            channel_count: 2,
            flags: AudioPortFlags::IS_MAIN,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: None,
        });
    }
}

// ---- Params ----
impl<'a> PluginMainThreadParams for CaveMainThread<'a> {
    fn count(&mut self) -> u32 { 1 }

    fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter) {
        if param_index != 0 { return; }

        info.set(&ParamInfo {
            id: ClapId::new(PARAM_GAIN_ID),
            flags: ParamInfoFlags::IS_AUTOMATABLE,
            cookie: Default::default(),
            name: b"Gain",
            module: b"",
            min_value: 0.0,
            max_value: 1.0,
            default_value: 0.5,
        });
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        match param_id.into() {
            PARAM_GAIN_ID => Some(self.shared.params.gain() as f64),
            _ => None,
        }
    }

    fn value_to_text(
        &mut self,
        _param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        use std::fmt::Write;
        write!(writer, "{:.3}", value)
    }

    fn text_to_value(&mut self, _param_id: ClapId, text: &CStr) -> Option<f64> {
        text.to_str().ok()?.parse::<f64>().ok()
    }

    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        for event in input {
            if let Some(CoreEventSpace::ParamValue(ev)) = event.as_core_event() {
                self.shared.params.handle_param_value_event(ev);
            }
        }
    }
}

impl<'a> PluginAudioProcessorParams for CaveAudioProcessor<'a> {
    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        for event in input {
            if let Some(CoreEventSpace::ParamValue(ev)) = event.as_core_event() {
                self.shared.params.handle_param_value_event(ev);
            }
        }
    }
}

// ---- GUI ----
impl<'a> PluginGuiImpl for CaveMainThread<'a> {
    fn is_api_supported(&mut self, cfg: GuiConfiguration) -> bool {
        #[cfg(target_os = "linux")]
        { cfg.api_type == GuiApiType::X11 && !cfg.is_floating }

        #[cfg(not(target_os = "linux"))]
        {
            let default = GuiApiType::default_for_current_platform().unwrap_or(GuiApiType::Win32);
            cfg.api_type == default && !cfg.is_floating
        }
    }

    fn get_preferred_api(&mut self) -> Option<GuiConfiguration> {
        #[cfg(target_os = "linux")]
        { Some(GuiConfiguration { api_type: GuiApiType::X11, is_floating: false }) }

        #[cfg(not(target_os = "linux"))]
        { Some(GuiConfiguration { api_type: GuiApiType::default_for_current_platform()?, is_floating: false }) }
    }

    fn create(&mut self, cfg: GuiConfiguration) -> Result<(), PluginError> {
        eprintln!("[cave-gui] create: {:?}", cfg);
        Ok(())
    }

    fn destroy(&mut self) {
        eprintln!("[cave-gui] destroy");
        self.gui.close();
    }

    fn set_scale(&mut self, scale: f64) -> Result<(), PluginError> {
        eprintln!("[cave-gui] set_scale: {}", scale);
        Ok(())
    }

    fn get_size(&mut self) -> Option<GuiSize> {
        Some(GuiSize { width: 400, height: 300 })
    }

    fn set_size(&mut self, size: GuiSize) -> Result<(), PluginError> {
        eprintln!("[cave-gui] set_size: {:?}", size);
        Ok(())
    }

    fn set_parent(&mut self, window: Window) -> Result<(), PluginError> {
        let h = window.raw_window_handle();
        eprintln!("[cave-gui] set_parent: {:?}", h);
        self.gui.parent = Some(h);

        if self.gui.is_open() {
            eprintln!("[cave-gui] already open, skip open()");
            return Ok(());
        }

        eprintln!("[cave-gui] opening GUI from set_parent()");
        self.gui.open(self.shared.params.clone())
    }

    fn set_transient(&mut self, _window: Window) -> Result<(), PluginError> {
        Ok(())
    }

    fn show(&mut self) -> Result<(), PluginError> {
        eprintln!("[cave-gui] show");
        if !self.gui.is_open() {
            self.gui.open(self.shared.params.clone())?;
        }
        Ok(())
    }

    fn hide(&mut self) -> Result<(), PluginError> {
        eprintln!("[cave-gui] hide");
        self.gui.close();
        Ok(())
    }
}

// MIDI Note to Frequency Helper
fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

clack_export_entry!(SinglePluginEntry<Cave>);
