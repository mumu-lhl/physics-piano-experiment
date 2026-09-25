//! Native CLAP Plugin Implementation in Rust with Host Thread Pool Integration.

use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::plugin::{clap_plugin, clap_plugin_descriptor};
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host;
use clap_sys::process::{clap_process, clap_process_status, CLAP_PROCESS_CONTINUE};
use clap_sys::version::CLAP_VERSION;
use clap_sys::id::{clap_id, CLAP_INVALID_ID};
use clap_sys::ext::thread_pool::{clap_host_thread_pool, clap_plugin_thread_pool, CLAP_EXT_THREAD_POOL};
use clap_sys::ext::audio_ports::{
    clap_plugin_audio_ports, clap_audio_port_info,
    CLAP_EXT_AUDIO_PORTS, CLAP_PORT_STEREO, CLAP_AUDIO_PORT_IS_MAIN, CLAP_AUDIO_PORT_SUPPORTS_64BITS,
};
use clap_sys::ext::note_ports::{
    clap_plugin_note_ports, clap_note_port_info,
    CLAP_EXT_NOTE_PORTS, CLAP_NOTE_DIALECT_CLAP, CLAP_NOTE_DIALECT_MIDI,
};
use clap_sys::ext::params::{
    clap_plugin_params, clap_param_info,
    CLAP_EXT_PARAMS, CLAP_PARAM_IS_AUTOMATABLE,
};
use clap_sys::events::{
    CLAP_EVENT_NOTE_ON, CLAP_EVENT_NOTE_OFF, CLAP_EVENT_NOTE_END,
    CLAP_EVENT_NOTE_EXPRESSION, CLAP_NOTE_EXPRESSION_TUNING,
    CLAP_EVENT_PARAM_VALUE, CLAP_EVENT_MIDI,
    clap_event_header, clap_event_note, clap_event_note_expression,
    clap_event_param_value, clap_event_midi,
};

use crate::engine::{PianoEngine, EngineEvent, EngineOutEvent};

fn copy_to_c_arr(dest: &mut [c_char], src: &str) {
    let bytes = src.as_bytes();
    let len = bytes.len().min(dest.len() - 1);
    for i in 0..len {
        dest[i] = bytes[i] as c_char;
    }
    dest[len] = 0;
}

pub static PLUGIN_DESCRIPTOR: clap_plugin_descriptor = clap_plugin_descriptor {
    clap_version: CLAP_VERSION,
    id: b"com.mumu.physics-piano\0".as_ptr() as *const c_char,
    name: b"Physics Piano\0".as_ptr() as *const c_char,
    vendor: b"mumu-lhl\0".as_ptr() as *const c_char,
    url: b"https://github.com/mumu-lhl/physics-piano-experiment\0".as_ptr() as *const c_char,
    manual_url: b"\0".as_ptr() as *const c_char,
    support_url: b"\0".as_ptr() as *const c_char,
    version: b"0.1.0\0".as_ptr() as *const c_char,
    description: b"First-principles physical modeling acoustic piano synthesizer\0".as_ptr() as *const c_char,
    features: [
        b"instrument\0".as_ptr() as *const c_char,
        b"synthesizer\0".as_ptr() as *const c_char,
        b"physical-modeling\0".as_ptr() as *const c_char,
        ptr::null(),
    ].as_ptr() as *const *const c_char,
};

pub struct PluginInstance {
    pub host: *const clap_host,
    pub engine: PianoEngine,
    pub host_thread_pool: Option<*const clap_host_thread_pool>,
    pub master_volume: f64,
    pub scratch_left: Vec<f64>,
    pub scratch_right: Vec<f64>,
    pub events_scratch: Vec<EngineEvent>,
    pub out_events_scratch: Vec<EngineOutEvent>,
}

// -----------------------------------------------------------------------------
// CLAP Plugin C-ABI Callback Implementations
// -----------------------------------------------------------------------------

unsafe extern "C" fn plugin_init(plugin: *const clap_plugin) -> bool {
    let instance = &mut *((*plugin).plugin_data as *mut PluginInstance);
    // Request Host Thread Pool extension if available
    if !instance.host.is_null() && !(*instance.host).get_extension.is_none() {
        let get_ext = (*instance.host).get_extension.unwrap();
        let ext = get_ext(instance.host, CLAP_EXT_THREAD_POOL.as_ptr() as *const c_char);
        if !ext.is_null() {
            instance.host_thread_pool = Some(ext as *const clap_host_thread_pool);
        }
    }
    true
}

unsafe extern "C" fn plugin_destroy(plugin: *const clap_plugin) {
    if !plugin.is_null() && !(*plugin).plugin_data.is_null() {
        let _ = Box::from_raw((*plugin).plugin_data as *mut PluginInstance);
    }
}

unsafe extern "C" fn plugin_activate(
    plugin: *const clap_plugin,
    sample_rate: f64,
    _min_frames_count: u32,
    max_frames_count: u32,
) -> bool {
    let instance = &mut *((*plugin).plugin_data as *mut PluginInstance);
    instance.engine = PianoEngine::new(sample_rate, 35, true);
    let buf_size = max_frames_count as usize;
    instance.scratch_left = vec![0.0; buf_size];
    instance.scratch_right = vec![0.0; buf_size];
    true
}

unsafe extern "C" fn plugin_deactivate(_plugin: *const clap_plugin) {}

unsafe extern "C" fn plugin_start_processing(_plugin: *const clap_plugin) -> bool {
    true
}

unsafe extern "C" fn plugin_stop_processing(_plugin: *const clap_plugin) {}

unsafe extern "C" fn plugin_reset(plugin: *const clap_plugin) {
    let instance = &mut *((*plugin).plugin_data as *mut PluginInstance);
    instance.engine.active_keys.clear();
    instance.engine.voices.clear();
}

unsafe extern "C" fn plugin_process(
    plugin: *const clap_plugin,
    process: *const clap_process,
) -> clap_process_status {
    let instance = &mut *((*plugin).plugin_data as *mut PluginInstance);
    let frames = (*process).frames_count as usize;

    if frames == 0 {
        return CLAP_PROCESS_CONTINUE;
    }

    if instance.scratch_left.len() < frames {
        instance.scratch_left.resize(frames, 0.0);
        instance.scratch_right.resize(frames, 0.0);
    }

    // 1. Stage 1: Decode CLAP sample-accurate event list
    instance.events_scratch.clear();
    instance.out_events_scratch.clear();

    let in_events = (*process).in_events;
    if !in_events.is_null() && !(*in_events).size.is_none() && !(*in_events).get.is_none() {
        let num_events = ((*in_events).size.unwrap())(in_events);
        for i in 0..num_events {
            let hdr = ((*in_events).get.unwrap())(in_events, i);
            if hdr.is_null() {
                continue;
            }
            let time = (*hdr).time as usize;
            match (*hdr).type_ {
                CLAP_EVENT_NOTE_ON => {
                    let note_ev = &*(hdr as *const clap_event_note);
                    let key = note_ev.key as u8;
                    let velocity = note_ev.velocity;
                    instance.events_scratch.push(EngineEvent::NoteOn { time, key, velocity });
                }
                CLAP_EVENT_NOTE_OFF => {
                    let note_ev = &*(hdr as *const clap_event_note);
                    let key = note_ev.key as u8;
                    instance.events_scratch.push(EngineEvent::NoteOff { time, key });
                }
                CLAP_EVENT_NOTE_EXPRESSION => {
                    let exp_ev = &*(hdr as *const clap_event_note_expression);
                    if exp_ev.expression_id == CLAP_NOTE_EXPRESSION_TUNING {
                        let key = exp_ev.key as u8;
                        let cents = exp_ev.value * 100.0;
                        instance.events_scratch.push(EngineEvent::NoteTuning { time, key, cents });
                    }
                }
                CLAP_EVENT_PARAM_VALUE => {
                    let p_ev = &*(hdr as *const clap_event_param_value);
                    match p_ev.param_id {
                        0 => {
                            instance.engine.set_sustain_pedal(p_ev.value > 0.01, p_ev.value);
                        }
                        1 => {
                            instance.engine.set_una_corda(p_ev.value > 0.5);
                        }
                        2 => {
                            instance.master_volume = p_ev.value.clamp(0.0, 2.0);
                        }
                        _ => {}
                    }
                }
                CLAP_EVENT_MIDI => {
                    let midi_ev = &*(hdr as *const clap_event_midi);
                    let status = midi_ev.data[0] & 0xF0;
                    let d1 = midi_ev.data[1];
                    let d2 = midi_ev.data[2];
                    match status {
                        0x90 => {
                            // Note On
                            if d2 > 0 {
                                instance.events_scratch.push(EngineEvent::NoteOn {
                                    time,
                                    key: d1,
                                    velocity: (d2 as f64) / 127.0,
                                });
                            } else {
                                instance.events_scratch.push(EngineEvent::NoteOff { time, key: d1 });
                            }
                        }
                        0x80 => {
                            // Note Off
                            instance.events_scratch.push(EngineEvent::NoteOff { time, key: d1 });
                        }
                        0xB0 => {
                            // Control Change
                            if d1 == 64 {
                                // CC 64 Damper Pedal
                                let depth = (d2 as f64) / 127.0;
                                instance.engine.set_sustain_pedal(d2 >= 64, depth);
                            } else if d1 == 67 {
                                // CC 67 Soft Pedal (Una Corda)
                                instance.engine.set_una_corda(d2 >= 64);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    // 2. Stage 2, 3, 4: Execute 4-stage pipeline
    instance.engine.process_block(
        frames,
        &instance.events_scratch,
        &mut instance.out_events_scratch,
        &mut instance.scratch_left[..frames],
        &mut instance.scratch_right[..frames],
    );

    // Apply master volume
    let vol = instance.master_volume;
    if (vol - 1.0).abs() > 1e-4 {
        for s in 0..frames {
            instance.scratch_left[s] *= vol;
            instance.scratch_right[s] *= vol;
        }
    }

    // 3. Output audio mixing to host audio_outputs
    if (*process).audio_outputs_count > 0 && !(*process).audio_outputs.is_null() {
        let out_ports = (*process).audio_outputs;
        let num_channels = (*out_ports).channel_count as usize;
        let data32 = (*out_ports).data32;
        let data64 = (*out_ports).data64;

        if !data32.is_null() {
            // 32-bit float output
            let ch_ptrs = std::slice::from_raw_parts(data32, num_channels);
            if num_channels >= 1 && !ch_ptrs[0].is_null() {
                let left_slice = std::slice::from_raw_parts_mut(ch_ptrs[0], frames);
                for f in 0..frames {
                    left_slice[f] = instance.scratch_left[f] as f32;
                }
            }
            if num_channels >= 2 && !ch_ptrs[1].is_null() {
                let right_slice = std::slice::from_raw_parts_mut(ch_ptrs[1], frames);
                for f in 0..frames {
                    right_slice[f] = instance.scratch_right[f] as f32;
                }
            }
        } else if !data64.is_null() {
            // 64-bit double output
            let ch_ptrs = std::slice::from_raw_parts(data64, num_channels);
            if num_channels >= 1 && !ch_ptrs[0].is_null() {
                let left_slice = std::slice::from_raw_parts_mut(ch_ptrs[0], frames);
                left_slice.copy_from_slice(&instance.scratch_left[..frames]);
            }
            if num_channels >= 2 && !ch_ptrs[1].is_null() {
                let right_slice = std::slice::from_raw_parts_mut(ch_ptrs[1], frames);
                right_slice.copy_from_slice(&instance.scratch_right[..frames]);
            }
        }
    }

    // 4. Emit Note End events for retired voices
    let out_events = (*process).out_events;
    if !out_events.is_null() && !(*out_events).try_push.is_none() {
        let try_push = (*out_events).try_push.unwrap();
        for ev in &instance.out_events_scratch {
            match ev {
                EngineOutEvent::NoteEnd { time, key } => {
                    let end_ev = clap_event_note {
                        header: clap_event_header {
                            size: std::mem::size_of::<clap_event_note>() as u32,
                            time: *time as u32,
                            space_id: 0,
                            type_: CLAP_EVENT_NOTE_END,
                            flags: 0,
                        },
                        note_id: -1,
                        port_index: 0,
                        channel: 0,
                        key: *key as i16,
                        velocity: 0.0,
                    };
                    try_push(out_events, &end_ev.header);
                }
            }
        }
    }

    CLAP_PROCESS_CONTINUE
}

// -----------------------------------------------------------------------------
// CLAP Extension: Audio Ports
// -----------------------------------------------------------------------------

unsafe extern "C" fn audio_ports_count(_plugin: *const clap_plugin, is_input: bool) -> u32 {
    if is_input { 0 } else { 1 }
}

unsafe extern "C" fn audio_ports_get(
    _plugin: *const clap_plugin,
    index: u32,
    is_input: bool,
    info: *mut clap_audio_port_info,
) -> bool {
    if is_input || index != 0 || info.is_null() {
        return false;
    }
    (*info).id = 0;
    copy_to_c_arr(&mut (*info).name, "Main Output");
    (*info).flags = CLAP_AUDIO_PORT_IS_MAIN | CLAP_AUDIO_PORT_SUPPORTS_64BITS;
    (*info).channel_count = 2;
    (*info).port_type = CLAP_PORT_STEREO.as_ptr();
    (*info).in_place_pair = CLAP_INVALID_ID;
    true
}

static PLUGIN_AUDIO_PORTS: clap_plugin_audio_ports = clap_plugin_audio_ports {
    count: Some(audio_ports_count),
    get: Some(audio_ports_get),
};

// -----------------------------------------------------------------------------
// CLAP Extension: Note Ports
// -----------------------------------------------------------------------------

unsafe extern "C" fn note_ports_count(_plugin: *const clap_plugin, is_input: bool) -> u32 {
    if is_input { 1 } else { 0 }
}

unsafe extern "C" fn note_ports_get(
    _plugin: *const clap_plugin,
    index: u32,
    is_input: bool,
    info: *mut clap_note_port_info,
) -> bool {
    if !is_input || index != 0 || info.is_null() {
        return false;
    }
    (*info).id = 0;
    (*info).supported_dialects = CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI;
    (*info).preferred_dialect = CLAP_NOTE_DIALECT_CLAP;
    copy_to_c_arr(&mut (*info).name, "Note In");
    true
}

static PLUGIN_NOTE_PORTS: clap_plugin_note_ports = clap_plugin_note_ports {
    count: Some(note_ports_count),
    get: Some(note_ports_get),
};

// -----------------------------------------------------------------------------
// CLAP Extension: Parameters
// -----------------------------------------------------------------------------

unsafe extern "C" fn params_count(_plugin: *const clap_plugin) -> u32 {
    3
}

unsafe extern "C" fn params_get_info(
    _plugin: *const clap_plugin,
    param_index: u32,
    info: *mut clap_param_info,
) -> bool {
    if info.is_null() {
        return false;
    }
    match param_index {
        0 => {
            (*info).id = 0;
            (*info).flags = CLAP_PARAM_IS_AUTOMATABLE;
            (*info).cookie = ptr::null_mut();
            copy_to_c_arr(&mut (*info).name, "Sustain Pedal");
            copy_to_c_arr(&mut (*info).module, "Pedals");
            (*info).min_value = 0.0;
            (*info).max_value = 1.0;
            (*info).default_value = 0.0;
            true
        }
        1 => {
            (*info).id = 1;
            (*info).flags = CLAP_PARAM_IS_AUTOMATABLE;
            (*info).cookie = ptr::null_mut();
            copy_to_c_arr(&mut (*info).name, "Una Corda");
            copy_to_c_arr(&mut (*info).module, "Pedals");
            (*info).min_value = 0.0;
            (*info).max_value = 1.0;
            (*info).default_value = 0.0;
            true
        }
        2 => {
            (*info).id = 2;
            (*info).flags = CLAP_PARAM_IS_AUTOMATABLE;
            (*info).cookie = ptr::null_mut();
            copy_to_c_arr(&mut (*info).name, "Master Volume");
            copy_to_c_arr(&mut (*info).module, "Master");
            (*info).min_value = 0.0;
            (*info).max_value = 2.0;
            (*info).default_value = 1.0;
            true
        }
        _ => false,
    }
}

unsafe extern "C" fn params_get_value(
    plugin: *const clap_plugin,
    param_id: clap_id,
    out_value: *mut f64,
) -> bool {
    if plugin.is_null() || out_value.is_null() {
        return false;
    }
    let instance = &*((*plugin).plugin_data as *const PluginInstance);
    match param_id {
        0 => {
            *out_value = if instance.engine.sustain_pedal { instance.engine.pedal_depth } else { 0.0 };
            true
        }
        1 => {
            *out_value = if instance.engine.una_corda { 1.0 } else { 0.0 };
            true
        }
        2 => {
            *out_value = instance.master_volume;
            true
        }
        _ => false,
    }
}

unsafe extern "C" fn params_value_to_text(
    _plugin: *const clap_plugin,
    param_id: clap_id,
    value: f64,
    out_buffer: *mut c_char,
    out_buffer_capacity: u32,
) -> bool {
    if out_buffer.is_null() || out_buffer_capacity == 0 {
        return false;
    }
    let text = match param_id {
        0 => format!("{:.0}%", value * 100.0),
        1 => if value > 0.5 { "On".to_string() } else { "Off".to_string() },
        2 => format!("{:.1} dB", 20.0 * value.max(1e-4).log10()),
        _ => return false,
    };
    let slice = std::slice::from_raw_parts_mut(out_buffer, out_buffer_capacity as usize);
    copy_to_c_arr(slice, &text);
    true
}

unsafe extern "C" fn params_text_to_value(
    _plugin: *const clap_plugin,
    _param_id: clap_id,
    _param_value_text: *const c_char,
    _out_value: *mut f64,
) -> bool {
    false
}

unsafe extern "C" fn params_flush(
    _plugin: *const clap_plugin,
    _in_events: *const clap_sys::events::clap_input_events,
    _out_events: *const clap_sys::events::clap_output_events,
) {}

static PLUGIN_PARAMS: clap_plugin_params = clap_plugin_params {
    count: Some(params_count),
    get_info: Some(params_get_info),
    get_value: Some(params_get_value),
    value_to_text: Some(params_value_to_text),
    text_to_value: Some(params_text_to_value),
    flush: Some(params_flush),
};

// -----------------------------------------------------------------------------
// CLAP Extension: Thread Pool
// -----------------------------------------------------------------------------

static PLUGIN_THREAD_POOL: clap_plugin_thread_pool = clap_plugin_thread_pool {
    exec: Some(thread_pool_exec),
};

unsafe extern "C" fn thread_pool_exec(_plugin: *const clap_plugin, _task_index: u32) {}

unsafe extern "C" fn plugin_get_extension(
    _plugin: *const clap_plugin,
    id: *const c_char,
) -> *const c_void {
    if id.is_null() {
        return ptr::null();
    }
    let id_str = CStr::from_ptr(id);
    if id_str.to_bytes() == CLAP_EXT_THREAD_POOL.to_bytes() {
        &PLUGIN_THREAD_POOL as *const _ as *const c_void
    } else if id_str.to_bytes() == CLAP_EXT_AUDIO_PORTS.to_bytes() {
        &PLUGIN_AUDIO_PORTS as *const _ as *const c_void
    } else if id_str.to_bytes() == CLAP_EXT_NOTE_PORTS.to_bytes() {
        &PLUGIN_NOTE_PORTS as *const _ as *const c_void
    } else if id_str.to_bytes() == CLAP_EXT_PARAMS.to_bytes() {
        &PLUGIN_PARAMS as *const _ as *const c_void
    } else {
        ptr::null()
    }
}

unsafe extern "C" fn plugin_on_main_thread(_plugin: *const clap_plugin) {}

// -----------------------------------------------------------------------------
// Plugin Factory & Entrypoint
// -----------------------------------------------------------------------------

unsafe extern "C" fn factory_get_plugin_count(_factory: *const clap_plugin_factory) -> u32 {
    1
}

unsafe extern "C" fn factory_get_plugin_descriptor(
    _factory: *const clap_plugin_factory,
    index: u32,
) -> *const clap_plugin_descriptor {
    if index == 0 {
        &PLUGIN_DESCRIPTOR
    } else {
        ptr::null()
    }
}

unsafe extern "C" fn factory_create_plugin(
    _factory: *const clap_plugin_factory,
    host: *const clap_host,
    plugin_id: *const c_char,
) -> *const clap_plugin {
    if plugin_id.is_null() {
        return ptr::null();
    }
    let id_str = CStr::from_ptr(plugin_id);
    if id_str.to_bytes() != b"com.mumu.physics-piano" {
        return ptr::null();
    }

    let instance = Box::new(PluginInstance {
        host,
        engine: PianoEngine::new(48000.0, 35, true),
        host_thread_pool: None,
        master_volume: 1.0,
        scratch_left: vec![0.0; 256],
        scratch_right: vec![0.0; 256],
        events_scratch: Vec::with_capacity(32),
        out_events_scratch: Vec::with_capacity(32),
    });

    let plugin = Box::new(clap_plugin {
        desc: &PLUGIN_DESCRIPTOR,
        plugin_data: Box::into_raw(instance) as *mut c_void,
        init: Some(plugin_init),
        destroy: Some(plugin_destroy),
        activate: Some(plugin_activate),
        deactivate: Some(plugin_deactivate),
        start_processing: Some(plugin_start_processing),
        stop_processing: Some(plugin_stop_processing),
        reset: Some(plugin_reset),
        process: Some(plugin_process),
        get_extension: Some(plugin_get_extension),
        on_main_thread: Some(plugin_on_main_thread),
    });

    Box::into_raw(plugin)
}

static PLUGIN_FACTORY: clap_plugin_factory = clap_plugin_factory {
    get_plugin_count: Some(factory_get_plugin_count),
    get_plugin_descriptor: Some(factory_get_plugin_descriptor),
    create_plugin: Some(factory_create_plugin),
};

#[no_mangle]
pub static clap_entry: clap_plugin_entry = clap_plugin_entry {
    clap_version: CLAP_VERSION,
    init: Some(entry_init),
    deinit: Some(entry_deinit),
    get_factory: Some(entry_get_factory),
};

unsafe extern "C" fn entry_init(_plugin_path: *const c_char) -> bool {
    true
}

unsafe extern "C" fn entry_deinit() {}

unsafe extern "C" fn entry_get_factory(factory_id: *const c_char) -> *const c_void {
    if factory_id.is_null() {
        return ptr::null();
    }
    let id_str = CStr::from_ptr(factory_id);
    if id_str.to_bytes() == CLAP_PLUGIN_FACTORY_ID.to_bytes() {
        &PLUGIN_FACTORY as *const _ as *const c_void
    } else {
        ptr::null()
    }
}
