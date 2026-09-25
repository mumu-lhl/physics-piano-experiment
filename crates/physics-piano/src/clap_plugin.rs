//! Native CLAP Plugin Implementation in Rust with Host Thread Pool Integration.

use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::plugin::{clap_plugin, clap_plugin_descriptor};
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host;
use clap_sys::process::{clap_process, clap_process_status, CLAP_PROCESS_CONTINUE};
use clap_sys::version::CLAP_VERSION;
use clap_sys::ext::thread_pool::{clap_host_thread_pool, clap_plugin_thread_pool, CLAP_EXT_THREAD_POOL};
use clap_sys::events::{
    CLAP_EVENT_NOTE_ON, CLAP_EVENT_NOTE_OFF, CLAP_EVENT_NOTE_END,
    CLAP_EVENT_NOTE_EXPRESSION, CLAP_NOTE_EXPRESSION_TUNING,
    clap_event_header, clap_event_note, clap_event_note_expression,
};

use crate::engine::{PianoEngine, EngineEvent, EngineOutEvent};

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
    instance.engine = PianoEngine::new(sample_rate, 35, false);
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

// Thread Pool Extension implementation
static PLUGIN_THREAD_POOL: clap_plugin_thread_pool = clap_plugin_thread_pool {
    exec: Some(thread_pool_exec),
};

unsafe extern "C" fn thread_pool_exec(_plugin: *const clap_plugin, _task_index: u32) {
    // Concurrent task execution for sliced voice modal stepping
}

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
        engine: PianoEngine::new(48000.0, 35, false),
        host_thread_pool: None,
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
