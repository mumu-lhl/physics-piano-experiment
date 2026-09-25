---
name: egui-audio-guidelines
description: >-
  Standard engineering guidelines and pitfall prevention for immediate-mode GUI (egui)
  in real-time audio plugins and synthesizers (CLAP, VST3, Standalone). Covers layout budgeting,
  column expansion clipping, slider sizing, lock-free audio/GUI synchronization, 60fps frame budgeting,
  and state lifecycle management.
---

# Egui Audio Plugin Development Guidelines & Pitfalls

Immediate-mode GUI (`egui`) renders UI procedurally every frame from application state rather than maintaining a persistent widget hierarchy. While this offers unparalleled iteration speed and eliminates complex GUI state-binding boilerplate, it introduces subtle layout and concurrency traps—especially in real-time audio plugins (NIH-plug, CLAP, VST3).

This guide documents the core pitfalls, rules of thumb, and standard code patterns required for all audio plugins in this repository.

---

## 1. Layout & Sizing Pitfalls

### Pitfall 1: Column Expansion and Right-Side Clipping
**The Problem:**
In `egui`, `ui.columns(n, |cols| { ... })` allocates an initial width of `available_width / n` for each column. However, egui containers expand to fit their children if child widgets request more space. If a widget in the rightmost column requests more width than allocated, egui expands that column to the right, pushing content **outside the window boundaries**. The host DAW or window manager clips this silently.

**Common Causes:**
1. Using `egui::Slider::new(...).text("Label")`: In `egui`, `.text("...")` appends a text label **to the right** of the slider bar. Together with the value display, the total widget width often exceeds `250px-300px`.
2. Setting `ui.set_width(ui.available_width())` inside a `ui.group(...)`: The group's internal margins add extra padding, leading to subtle expansion.

**Standard Solution: The Top-Label Pattern & Explicit Slider Sizing**
Always place the label **above** the slider using `ui.label(...)`, and size the slider explicitly to fit the column:

```rust
// ✅ CORRECT: Label on top, slider strictly bounded
ui.vertical(|ui| {
    ui.label(RichText::new("Master Gain").color(Color32::from_rgb(180, 185, 200)));
    let slider_width = (ui.available_width() - 4.0).max(60.0);
    ui.add(ParamSlider::for_param(&params.master_gain, setter).with_width(slider_width));
});

// ❌ WRONG: Inline label appends to the right, overflowing column boundaries
ui.add(egui::Slider::new(&mut gain_db, -30.0..=6.0).text("Master Gain (dB)"));
```

### Pitfall 2: `ui.columns` Overlap vs Sequential Horizontal Rack
**The Problem:**
In `egui`, `ui.columns(n, |cols| { ... })` allocates static coordinates for each column upfront ($x_i = x_0 + i \cdot (w + s)$). If any column's content expands (e.g., child groups, radio groups, sliders), or if `ui.set_width(...)` is called inside a column's `ui.group(...)`, egui paints that column's frame rectangle into adjacent columns, causing visual overlap (e.g., Column 2 overlapping Column 3).

**Standard Solution: Unified Chassis with Sequential Horizontal Sections**
Wrap the entire control rack in a single outer `ui.group`, and layout sections sequentially using `ui.horizontal` and `ui.separator()`:

```rust
// ✅ CORRECT: Sequential placement guarantees ZERO possibility of overlap
ui.group(|ui| {
    ui.set_width(ui.available_width());
    ui.horizontal(|ui| {
        let total_spacing = 3.0 * 20.0 + 20.0;
        let section_width = ((ui.available_width() - total_spacing) / 4.0).max(180.0);

        // Section 0
        ui.vertical(|ui| {
            ui.set_width(section_width);
            // ...
        });

        ui.separator();

        // Section 1
        ui.vertical(|ui| {
            ui.set_width(section_width);
            // ...
        });

        ui.separator();

        // Section 2
        ui.vertical(|ui| {
            ui.set_width(section_width);
            // ...
        });

        ui.separator();

        // Section 3
        ui.vertical(|ui| {
            ui.set_width(section_width);
            // ...
        });
    });
});
```

### Pitfall 3: NIH-Plug `ParamSlider` vs `egui::Slider`
For audio plugin parameters connected to `nih_plug::prelude::*Param`:
- **Prefer `nih_plug_egui::widgets::ParamSlider`**:
  - Automatically manages DAW parameter gestures (`setter.begin_set_parameter`, `setter.set_parameter`, `setter.end_set_parameter`).
  - Displays the formatted parameter value and units (e.g., `-6.0 dB`, `15 %`, `440.0 Hz`) **inside** the slider bar.
  - Supports double-click / right-click for direct numerical typing.
  - Supports `.with_width(w)` to guarantee zero overflow.

### Pitfall 4: Stale Persisted Window Geometry
When using `#[persist = "editor-state-vX"]` with `EguiState`:
- DAWs and standalone wrappers cache the window width and height between sessions.
- If you refactor a 3-column layout into a 4-column layout, existing host caches will open the plugin at the old smaller width, clipping the new UI!
- **Rule:** Whenever you alter the default window width, minimum size, or rack layout, **bump the persistence key version**:
  ```rust
  #[persist = "editor-state-v5"] // Incremented
  pub editor_state: Arc<EguiState>,
  ```

### Pitfall 5: CJK Font Loading for Internationalization (i18n)
When rendering non-Latin glyphs (Simplified Chinese, Japanese, Korean):
- By default, egui's built-in fonts only include Latin characters. Any Chinese characters will render as tofu boxes (`□□□`).
- **Solution:** In the `create_egui_editor` setup closure, scan system CJK font paths (`NotoSansCJK`, `DroidSansFallbackFull`, `wqy-microhei`, `LXGWWenKai`, `msyh.ttc`, `PingFang.ttc`) and insert the font bytes into `FontDefinitions::default().font_data` as a fallback for both `FontFamily::Proportional` and `FontFamily::Monospace`.
- Provide a `Language` enum and language toggle button in the header banner.

---

## 2. Real-Time Audio Thread & GUI Concurrency

### Pitfall 4: Audio Thread Lock Contention
The audio processing callback (`process()`) runs in hard real-time under high-priority OS threads (e.g. ALSA/JACK/CoreAudio/ASIO).

**Strict Rules:**
1. **Never use blocking locks** (`std::sync::Mutex`, `RwLock::write`) inside the audio thread for UI interaction.
2. For parameters, use NIH-plug's native `Param` descriptors.
3. For visualization (peak meters, active notes, string vibration energy, FFT scopes):
   - Use **atomic variables** (`AtomicU8`, `AtomicBool`, `AtomicU32` storing `f32::to_bits()`).
   - For complex data streams (e.g. waveform orbits, oscilloscope buffers), use lock-free ringbuffers (`crossbeam_channel::bounded`, `triple_buffer`, or read-biased `parking_lot::RwLock`).
   - Use `Ordering::Relaxed` for meters and visual states where a single-frame tear is harmless.

```rust
// ✅ CORRECT: Atomic exchange of float bits between audio thread & GUI
let energy_f32 = (energy * 1000.0).clamp(0.0, 1.0) as f32;
shared_energy[i].store(energy_f32.to_bits(), Ordering::Relaxed);

// In GUI thread:
let energy = f32::from_bits(shared_energy[i].load(Ordering::Relaxed));
```

---

## 3. Interaction & Input State Lifecycles

### Pitfall 5: Stuck Note Highlights on Mouse Release
**The Problem:**
When building interactive musical widgets (fretboards, piano keyboards, XY pads), tracking note-on and note-off based on transient frame input flags (e.g., `i.pointer.primary_released()`) can fail:
- If the user clicks a key/fret and drags their mouse outside the widget or plugin window before releasing the button.
- If the pointer event is dropped due to window focus changes.
Result: The note stays held indefinitely, and the visual highlight never turns off.

**Standard Pattern:**
Check current continuous pointer state (`i.pointer.primary_down()`) and widget bounds. If the mouse button is not currently held down or the pointer is outside the interactive area, immediately release the held state:

```rust
let is_primary_down = ui.input(|i| i.pointer.primary_down());
let pointer_pos = ui.input(|i| i.pointer.latest_pos());

let mut hovered_target = None;
if let Some(pos) = pointer_pos {
    if rect.contains(pos) {
        hovered_target = Some(calculate_key_or_fret(pos));
    }
}

// Held target is ONLY valid while the primary button is actively pressed down
let target_held = if is_primary_down { hovered_target } else { None };

if *held_state != target_held {
    if let Some(old_note) = held_state.take() {
        on_released(old_note);
    }
    if let Some(new_note) = target_held {
        on_pressed(new_note);
        *held_state = Some(new_note);
    }
}
```

---

## 4. Physical Model Decay & Denormal Flushing

### Pitfall 6: Infinite Visual Glow and Denormals
In physical modeling synthesis (modal oscillators, strings, plates, soundboards), modal amplitudes decay exponentially: $q(t) = q_0 e^{-\gamma t}$.
1. **Denormals:** When $q(t) < 10^{-30}$, CPU x86/ARM microcode enters denormal floating-point emulation, causing 10x-100x CPU spikes in the audio thread!
2. **Visual Clamping:** If visual glow or meters activate whenever `energy > 0.02`, an undamped or slowly decaying mode ($\gamma \approx 0.5$) will remain lit for dozens of seconds, appearing permanently stuck.

**Standard Fix:**
1. **Active Release Damping:** When a note is released (`!string.is_held`), apply exponential muting damping (e.g. `0.997` per sample $\approx 150\text{ms}$ decay).
2. **Hard State Zeroing:** When total amplitude drops below $10^{-7}$, set all modal $q, v = 0$ to guarantee zero denormals and zero residual energy.
3. **Smooth Visual Fadeout:**
   ```rust
   if energy > 0.05 {
       let norm = ((energy - 0.05) / 0.95).clamp(0.0, 1.0);
       let glow_alpha = (norm * 220.0) as u8;
       // ... render glow
   }
   ```

---

## 5. Continuous Repainting & CPU Budget

### Pitfall 7: Unthrottled Repaint Loops
Calling `ctx.request_repaint()` unconditionally every frame forces the GUI to re-render at the display's maximum refresh rate (60Hz, 120Hz, 144Hz, or 240Hz ProMotion), consuming 5-15% of a CPU core even when the plugin is idle!

**Best Practice:**
- When visual elements are static (no keys pressed, audio silent, meters at zero), do not request repaint. Let egui sleep until the user moves the mouse or a parameter changes.
- When animating (audio playing or meters active):
  ```rust
  let is_animating = string_energies.iter().any(|&e| e > 0.001) || peak_meter > 0.001;
  if is_animating {
      egui_ctx.request_repaint();
  }
  ```
