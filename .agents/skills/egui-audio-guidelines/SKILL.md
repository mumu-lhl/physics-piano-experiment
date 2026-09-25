---
name: egui-audio-guidelines
description: >-
  Standard engineering guidelines and pitfall prevention for immediate-mode GUI (egui)
  in real-time audio plugins and synthesizers (CLAP, VST3, Standalone). Covers layout budgeting,
  column expansion clipping, slider sizing, lock-free audio/GUI synchronization, 60fps frame budgeting,
  and state lifecycle management. This skill is a living document; agents and developers are explicitly
  permitted and encouraged to modify, refine, and expand it whenever additions or corrections are needed.
---

# Egui Audio Plugin Development Guidelines & Pitfalls

Immediate-mode GUI (`egui`) renders UI procedurally every frame from application state rather than maintaining a persistent widget hierarchy. While this offers unparalleled iteration speed and eliminates complex GUI state-binding boilerplate, it introduces subtle layout and concurrency traps—especially in real-time audio plugins (NIH-plug, CLAP, VST3).

This guide documents the core pitfalls, rules of thumb, and standard code patterns required for all audio plugins in this repository.

> [!IMPORTANT]
> **Living Document & Modification Policy (文档动态演进与修改许可)**
> 本 Skill 是持续演进的动态工程规范。任何 AI Agent 或开发人员在开发、调试或重构 egui 界面时：
> - **允许并鼓励自由修改**：如果发现本 Skill 存在遗漏、描述不准确、与新版本 egui/NIH-plug 不兼容，或在实践中摸索出了更优的排版与并发解决方案，**无需额外申请许可，直接修改并更新此 Skill 文档**。
> - **持续扩充常见陷阱**：遇到任何新的 egui 疑难杂症（如 DPI 缩放异常、文本折行裁切、焦点捕获陷阱、高刷屏卡顿等），请将“问题复现、底层机制、标准修复代码”沉淀到本文档中。
> - **双向镜像同步**：修改后请确保 `.agents/skills/egui-audio-guidelines/SKILL.md` 与 `skills/egui-audio-guidelines/SKILL.md` 保持完全一致。

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

### Pitfall 2: `ui.columns` Overlap vs Sequential Horizontal Rack & Column Over-budgeting
**The Problem:**
1. In `egui`, `ui.columns(n, |cols| { ... })` allocates static coordinates for each column upfront ($x_i = x_0 + i \cdot (w + s)$). If any column's content expands, egui paints that column's frame into adjacent columns, causing severe visual overlap.
2. Even in a sequential `ui.horizontal` rack, calling `ui.set_width(w)` only sets `min_rect.max.x`. If a child widget (e.g. a long label, radio group, or checkbox) has an intrinsic minimum width $> w$, the column expands to the right!
3. If too many columns are placed side-by-side (e.g. 5 columns each claiming $\ge 180\text{px} + \text{spacing} = 984\text{px}$), any window narrower than 1000px will push the rightmost section (e.g. Master Output) **completely outside the window boundary**, causing silent clipping.

**Standard Solution: Unified Chassis with 4 Responsive Sections & Explicit `set_max_width`**
Wrap the entire control rack in a single outer `ui.group`, limit horizontal sections to $\le 4$ columns per row, and strictly bound both minimum and maximum section widths:

```rust
// ✅ CORRECT: Sequential placement + set_max_width guarantees ZERO clipping and ZERO overlap
ui.group(|ui| {
    ui.set_width(ui.available_width());
    ui.horizontal(|ui| {
        let total_spacing = 3.0 * 16.0 + 20.0;
        // Clamp dynamically to ensure the total width never overflows available space
        let section_width = ((ui.available_width() - total_spacing) / 4.0).clamp(160.0, 260.0);
        let slider_w = (section_width - 8.0).max(60.0);

        // Section 0
        ui.vertical(|ui| {
            ui.set_width(section_width);
            ui.set_max_width(section_width); // Prevents child widgets from pushing column wider!
            // ...
        });

        ui.separator();

        // Section 1
        ui.vertical(|ui| {
            ui.set_width(section_width);
            ui.set_max_width(section_width);
            // ...
        });

        ui.separator();

        // Section 2
        ui.vertical(|ui| {
            ui.set_width(section_width);
            ui.set_max_width(section_width);
            // ...
        });

        ui.separator();

        // Section 3 (Rightmost section - perfectly fits within window)
        ui.vertical(|ui| {
            ui.set_width(section_width);
            ui.set_max_width(section_width);
            ui.add(ParamSlider::for_param(&params.master_gain, setter).with_width(slider_w));
        });
    });
});
```

### Pitfall 3: NIH-Plug `ParamSlider` Internal Width & Value Box Offset
**The Trap:**
In `nih_plug_egui::widgets::ParamSlider`, `.with_width(w)` sets **only the width of the draggable slider bar**, NOT the total widget width!
Internally, `ParamSlider::ui` is implemented as:
```rust
ui.horizontal(|ui| {
    // 1. Draggable slider bar of width `w`
    ui.vertical(|ui| { ui.allocate_response(vec2(slider_width, height), ...); });
    // 2. Value display box (e.g. "0.0 dB", "15 %", "440.0 Hz") appended to the RIGHT!
    if self.draw_value {
        self.value_ui(ui); // Consumes an additional ~55px - 65px!
    }
});
```
If you set `let slider_w = (column_width - 8.0)`, the total widget width becomes `column_width + 55px`, which silently overflows the column! In the rightmost column of a rack (e.g. Master Output), this pushes the value box and the slider handle **completely off the right screen border**.

**Standard Sizing Rule:**
Always reserve at least `70.0px` for the value box and internal padding:
```rust
// ✅ CORRECT: Total widget width (slider_w + 60px) fits perfectly within section_width
let slider_w = (section_width - 70.0).clamp(60.0, 160.0);
ui.add(ParamSlider::for_param(&params.master_gain, setter).with_width(slider_w));

// ❌ WRONG: Value box overflows column by ~50px, causing right-side clipping
let slider_w = (section_width - 8.0).max(60.0);
ui.add(ParamSlider::for_param(&params.master_gain, setter).with_width(slider_w));
```

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

---

## 6. Skill Maintenance, Refinement & Permission to Modify (规范维护与动态修订指南)

### 明确修改授权 (Explicit Permission to Modify)
本 Skill 绝非一成不变的僵化教条。**项目允许并明确授权所有协作者与 AI Agent 在遇到以下情况时直接修改、纠错或扩充本 Skill 文件**：

1. **发现新陷阱 (New Pitfalls Discovered)**:
   - 在开发新的物理建模合成器控件（如包络绘制器、LFO 曲线面板、3D 空间声相网格、滤波频谱图等）时，踩到了新的 egui 布局、渲染或事件处理陷阱，并在解决后提炼出了通用解法。
2. **规范存在问题或边缘漏洞 (Issues or Incomplete Rules)**:
   - 现存的某项建议或规避策略在特定宿主（DAW，如 Reaper, Bitwig, Ableton, FL Studio, Logic）或操作系统（Linux Wayland vs X11, macOS Retina 缩放, Windows HiDPI 缩放）下表现不完美，需要修正、补充例外条件或调整推荐方案。
3. **上游框架版本迭代 (Upstream API & Framework Updates)**:
   - 依赖的 `egui` 或 `nih_plug_egui` 版本升级，引入了更优秀的原生布局机制（如自适应 Grid、改良的 ScrollArea 行为、更轻量级的动画回调）或弃用了旧 API。

### 修订工作流规范 (Editing Workflow)
- **分析机制并给出根因**：记录陷阱时，需阐明其在即时模式（immediate-mode）单遍渲染机制下的深层原因，而非仅仅贴出一段临时补丁。
- **保留正反代码对照**：始终保留清晰的 `// ✅ CORRECT` 与 `// ❌ WRONG` 代码段，供未来的开发者与 AI Agent 快速检索学习。
- **双向同步更新**：本仓库遵循双路径技能发现规范，修改后必须同步两处副本：
  ```bash
  cp .agents/skills/egui-audio-guidelines/SKILL.md skills/egui-audio-guidelines/SKILL.md
  ```

