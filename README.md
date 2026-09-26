# Physics Piano & Guitar Synthesizer

A high-performance, first-principles physical modeling acoustic piano and guitar virtual instrument in **Rust** (with native CLAP / VST3 plugin bundles and standalone desktop applications), accompanied by an offline Python validation, calibration, and differentiable physics harness.

This project implements complete continuous mechanical simulations of acoustic and electric instruments, translating Euler-Bernoulli wave mechanics, Hunt-Crossley felt impact dynamics, Kirchhoff-Carrier geometric tension modulation, anisotropic bridge admittance, and soundboard/body radiation into high-performance discrete state-space synthesis engines with hard real-time execution guarantees.

---

## Architecture & Workspaces

The repository is organized as a Cargo workspace with distinct specialized crates alongside an offline research toolchain:

```
physics-piano-experiment/
├── Cargo.toml                    # Root workspace configuration
├── crates/
│   ├── physics-piano/            # Concert Grand Piano physical modeling engine & plugins
│   │   ├── src/bin/              # Standalone playable app & headless CLI
│   │   ├── src/core/             # Stiff string, SAV hammer, bridge, action noise, pedals
│   │   ├── src/dsp/              # Multi-perspective UPOLS convolution & lid opening baffle
│   │   ├── src/gui/              # Hardware-accelerated egui interface & 88-key keyboard
│   │   ├── src/params/           # 88-key grand piano physical parameter generator
│   │   ├── src/clap_plugin.rs    # Low-level CLAP C-ABI bindings
│   │   └── src/nih_plugin.rs     # NIH-plug wrapper (CLAP, VST3, Standalone)
│   └── physics-guitar/           # Acoustic & Electric Guitar physical modeling engine
│       ├── src/bin/              # Guitar standalone playable app & CLI
│       ├── src/core/             # Guitar string, tension modulation, body radiation, pluck
│       ├── src/gui/              # Real-time fretboard tracking & string energy visualizer
│       ├── src/presets.rs        # 6 factory presets (Martin D-28, Strat, Les Paul, etc.)
│       └── src/nih_plugin.rs     # Native CLAP / VST3 / Standalone guitar plugin
├── src/physics_piano/            # Python research harness & differentiable auto-voicing
├── xtask/                        # Build automation runner for plugin bundling (`cargo xtask bundle`)
└── tests/                        # Comprehensive Python and Rust integration test suites
```

---

## Key Features

### 1. Concert Grand Piano Engine (`crates/physics-piano`)
* **Euler-Bernoulli Stiff String Dynamics**: Fourth-order spatial dispersion with exact continuous-to-discrete matrix exponential transitions ($\Phi, \Gamma$) for unconditional numerical stability and zero frequency warping.
* **Dual-Polarization & Two-Stage Decay**: Independent vertical ($u_T$) and horizontal ($u_P$) transverse polarizations with anisotropic bridge mobility ($\text{Re}(Y_{TT}) \gg \text{Re}(Y_{PP})$), faithfully reproducing the characteristic *Prompt Sound* and *Aftersound* decay rates.
* **Hunt-Crossley & SAV Contact Dynamics**: Nonlinear felt compression with velocity-dependent contact duration contraction. Employs the **Scalar Auxiliary Variable (SAV)** energy quadratisation scheme ($\xi(t) = \sqrt{\frac{K_h}{p+1} \eta^{p+1}}$) to strictly guarantee unconditional energy dissipation and prevent numerical explosion.
* **Sympathetic Resonance & Damper Mechanics**: Continuous bridge energy feedback dynamically drives all undamped strings across the 88-key soundboard matrix when the sustain pedal is engaged.
* **Micro-Mechanical Action & Pedal Noise Suite**:
  - Key-bottom thump (key lever hitting felt punchings & keybed modes at 72 Hz, 135 Hz, 240 Hz).
  - Escapement jack let-off click during soft pianissimo playing and key-up back-rail clack.
  - 88-damper simultaneous lift acoustic whoosh upon sustain pedal depression.
  - Cast-iron plate / frame shock resonance (58 Hz, 165 Hz, 340 Hz) upon rapid pedal stomp.
  - Descending damper felt friction buzzing on vibrating strings upon note release.
* **Spatial Multi-Microphone Radiation & Lid Opening Baffle**:
  - Multi-perspective zero-allocation UPOLS convolution engine with 3 listening perspectives: **Close** (hammer rail), **Player** (binaural HRTF), and **Ambient** (Decca tree diffuse hall).
  - Continuously adjustable grand piano lid baffle: Closed ($0^\circ$), Half-stick ($15^\circ$), Full-stick ($45^\circ$), to Lid Removed ($60^\circ$).
* **Extreme Real-Time Performance & High Polyphony**:
  - 32-voice concurrent polyphony with lowest-energy released-first voice stealing.
  - Register-adaptive psychoacoustic Nyquist modal culling (scaling from 35 modes down to 6 in the high treble, slashing treble compute load by up to 70%).
  - Zero-heap allocation in the real-time audio callback (`process_block`).

### 2. Acoustic & Electric Guitar Engine (`crates/physics-guitar`)
* **Kirchhoff-Carrier Nonlinear Tension Modulation**:
  $$\Delta T(t) = \frac{EA\pi^2}{4L^2} \sum_n n^2 (q_{T,n}^2 + q_{P,n}^2)$$
  Dynamically models the instantaneous longitudinal stretching of the string under large transverse displacement, reproducing the authentic attack twang and dynamic pitch settling (Pitch Glide).
* **Dual-Polarization Bridge Dynamics**: Horizontal shear coupled with ~18% bridge rocking, low in-plane radiation loss delivering organic two-stage acoustic decay.
* **All-String Sympathetic Resonance**: Bridge velocity feedback drives open resting strings across the guitar soundboard and body cavity.
* **Adaptive Pluck & Tactile Snap**: String fundamental-dependent release impedance $|S(\omega)|$ and celluloid pick tactile snap impulse.
* **Stereo Acoustic Radiation & Body Modes**: Christensen soundhole center radiation, upper/lower bout binaural radiation, and dual allpass wood dispersion.
* **Factory Presets & Real-Time GUI**:
  - 6 Built-in Factory Presets: *Martin D-28 Fingerstyle*, *Dreadnought Strummer*, *Strat Clean Chime*, *Texas Blues Breakup*, *Les Paul Warm Jazz*, and *Flamenco Con Brio*.
  - Hardware-accelerated egui interface featuring real-time fretboard tracking, active fret indicators, string energy meters, and parameter racks.

---

## Quickstart: Building & Running

### Prerequisites
- **Rust Toolchain**: `stable` (MSRV 1.75+)
- **System Libraries (Linux)**:
  ```bash
  # Debian / Ubuntu
  sudo apt-get install libasound2-dev libgl1-mesa-dev libx11-xcb-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
  # Fedora
  sudo dnf install alsa-lib-devel mesa-libGL-devel libX11-devel libxcb-devel libxkbcommon-devel
  ```

### 1. Standalone Playable Desktop Applications
Launch the standalone instruments with native hardware-accelerated GUI (ALSA/JACK/CoreAudio/WASAPI backend support, playable with mouse or MIDI keyboard without a DAW):

```bash
# Launch Concert Grand Piano
cargo run --release -p physics-piano --bin physics-piano-standalone

# Launch Acoustic & Electric Guitar
cargo run --release -p physics-guitar --bin physics-guitar-standalone
```

### 2. Building & Bundling CLAP / VST3 Plugins for DAWs
Build production-ready native plugin bundles using the built-in `xtask` bundler:

```bash
# Bundle Piano plugin (outputs target/bundled/physics-piano.clap)
cargo xtask bundle physics-piano --release

# Bundle Guitar plugin (outputs target/bundled/physics-guitar.clap)
cargo xtask bundle physics-guitar --release
```

**Plugin Installation Paths**:
- **Linux**: Copy `.clap` bundles to `~/.clap/` or `/usr/lib/clap/`
- **macOS**: Copy `.clap` bundles to `~/Library/Audio/Plug-Ins/CLAP/`
- **Windows**: Copy `.clap` bundles to `C:\Program Files\Common Files\CLAP\`

### 3. Headless Audio Rendering (CLI)
Synthesize audio notes directly from the command line for headless rendering or batch processing:

```bash
# Piano: render single note A4 (440 Hz) for 3.0 seconds
cargo run --release -p physics-piano --bin physics-piano-rs -- render A4 3.0 piano_a4.wav 0.85

# Piano: run high-throughput performance benchmark
cargo run --release -p physics-piano --bin physics-piano-rs -- benchmark 35

# Guitar: render open E2 note
cargo run --release -p physics-guitar --bin physics-guitar-cli -- render E2 3.0 guitar_e2.wav 0.85
```

### 4. Running Workspace Test Suite
Verify physical invariants, numerical stability, and audio DSP integration across all crates:

```bash
cargo test --workspace
```

---

## Python Research Harness & Offline Voicing Calibration

The `src/physics_piano/` Python package serves as an offline research and verification environment for acoustic metric auditing and differentiable auto-voicing.

### Installation
```bash
# Using uv (recommended) or standard pip
uv pip install -e .
# or: pip install -e .
```

### Automated Voicing Calibration (Tier 9 Auto-Voicer)
The auto-voicing pipeline performs inverse physical parameter estimation against target audio recordings (e.g. Steinway Model B samples) using Multi-Resolution STFT Spectral Loss ($L_{\mathrm{MRSL}}$) and Nelder-Mead optimization:

```bash
python examples/auto_voicing_demo.py
```
Calibrated physical parameter multipliers are exported directly to `calibrated_voicing_A4.json`.

### Objective Acoustic Metrics Verification
```bash
# Inharmonicity B factor regression test
physics-piano verify inharmonicity --note A4

# Schroeder EDC two-stage decay test (Prompt vs. Aftersound)
physics-piano verify edc --note A4

# Nonlinear hammer contact duration ladder test
physics-piano verify contact --note A4

# Run all verification tests
physics-piano verify all --note A4
```

---

## Continuous Integration & Release (GitHub Actions)

The project includes an automated matrix CI/CD pipeline (`.github/workflows/ci.yml`):
- **Cross-Platform Matrix**: Automated compile and test passes on Linux (`ubuntu-latest`), macOS (`macos-latest`), and Windows (`windows-latest`) on every push and PR.
- **CLAP Plugin Bundles**: Cross-compiles and packages native `.clap` virtual instruments across all platforms.
- **Automated Releases**: Pushing a version tag (`git tag v0.1.0 && git push origin v0.1.0`) triggers a GitHub Release with multi-platform `.clap` archive downloads.

---

## Comprehensive Roadmap

### Tier 1: Experimental Prototype (Python Core) - [COMPLETED]
- [x] **Modal State-Space Stiff String Engine**: Euler-Bernoulli fourth-order dispersion equation solved via exact continuous-to-discrete matrix exponential transitions ($\Phi, \Gamma$).
- [x] **Hunt-Crossley Nonlinear Felt Impact**: Hysteretic compression model $F_h(t) = \max\left(0, K_h [\eta]^p + \lambda_h [\eta]^p \frac{d\eta}{dt}\right)$ with velocity-dependent contact duration contraction.
- [x] **Bridge Anisotropic Mobility**: Cross-polarization coupling ($Y_{TP} \neq 0$) inducing dual-polarization energy transfer and biexponential decay (Prompt sound vs. Aftersound).
- [x] **Unison Trichord Dynamics**: 3-string unison groups with micro-detuning interference and beating.
- [x] **Damper Network & Sympathetic Bus**: Viscoelastic damping and global bridge-driven sympathetic resonance under sustain pedal.
- [x] **88-Key Parameter Generator**: Continuous parameter scaling across full concert grand keyboard (A0 to C8).
- [x] **Unified CLI & API**: Note/chord rendering and automated objective acoustic inspection (`physics-piano render/verify/benchmark`).
- [x] **Objective Metric Test Suite**: Automated verification for inharmonicity regression ($\epsilon_B \le 1.2\%$, $\sigma(\Delta C) < 2.0$ cents) and Schroeder EDC decay ratio.

---

### Tier 2: Advanced Continuous Continuum Mechanics & Nonlinearities - [COMPLETED]
- [x] **Large-Amplitude Geometric Nonlinearity (Phantom Partials)**:
  - Formulated string geometric stretching strain: $\epsilon(t) = \frac{1}{2L} \int_0^L \left(\frac{\partial u}{\partial x}\right)^2 dx$.
  - Implemented dynamic longitudinal tension modulation: $\Delta T(t) = \frac{EA\pi^2}{4L^2} \sum_n n^2 (q_{T,n}^2 + q_{P,n}^2)$.
  - Coupled longitudinal boundary wave force $F_L$ with bridge admittance tensor component $Y_{TL}$, reproducing metallic phantom partials during fortissimo strikes.
- [x] **Orthotropic 2D Reissner-Mindlin Soundboard Continuous Simulation**:
  - Implemented 3D anisotropic admittance tensor $\mathbf{Y}_{bridge}$ ($V_T, V_P, V_L$).
  - Implemented Partitioned Uniform-Power Overlap-Save (UPOLS) FFT block convolution for real-time soundboard radiation.
- [x] **Implicit Energy-Preserving Contact & Bridge Coupling (SAV Scheme)**:
  - Implemented Scalar Auxiliary Variable (SAV) unconditionally energy-stable contact mechanics $\xi(t) = \sqrt{\frac{K_h}{p+1} \eta^{p+1}}$ ensuring unconditional numerical stability.
- [x] **Discontinuous 88-Key Scale Calibration**:
  - Incorporates empirical scale breaks: single-wound copper strings (A0-E1), double-wound strings (F1-Bb2), and plain steel wire transition (B2-C8).
  - Railsback stretch tuning curve with sub-bass flat offset and high treble sharp stretch.

---

### Tier 3: High-Performance Real-Time Engine in Rust - [COMPLETED]
- [x] **Core DSP Engine Migration**:
  - Full implementation in Rust (`crates/physics-piano/`).
  - Zero-heap allocation during realtime audio processing loop (`process_block`).
  - Hard real-time execution achieving RTF $\approx 0.06\times$ (16x faster than real-time).
- [x] **Voice Lifecycle & Energy-Driven Garbage Collection**:
  - Continuous modal energy monitoring $E_{total} = \frac{1}{2} \mu L \sum_n (v_n^2 + \omega_n^2 q_n^2)$.
  - Dynamic voice release when energy drops below -96 dB relative to note peak, emitting `CLAP_EVENT_NOTE_END`.

---

### Tier 4: Native CLAP Plugin Architecture - [COMPLETED]
- [x] **Host Collaborative Thread Pool Integration (`clap_host_thread_pool`)**:
  - Implemented `clap_plugin_thread_pool` interface with Rayon work-stealing fallback for standalone hosts.
  - Eliminates OS priority inversion and lock contention under low buffer sizes (64 samples).
- [x] **Sample-Accurate Modulation & Note Expressions**:
  - Sample-accurate event dispatch with sub-block timestamp indexing.
  - Per-note tuning modulation (`CLAP_NOTE_EXPRESSION_TUNING`) for dynamic microtonal adjustments.
  - Continuous damper pedal articulation (half-pedaling $[0, 1]$) and Una Corda soft-pedal felt strike displacement.
- [x] **CLAP C-ABI Export**:
  - `clap_entry` symbol exported from `libphysics_piano.so` (`cdylib`).

---

### Tier 5: Automated Objective CI/CD Acoustic Regression Pipeline - [COMPLETED]
- [x] **Physical Acoustic Residual Metrics**:
  - Dynamic Spectral Centroid scaling: $\kappa_{dyn} \in [0.45, 0.65]$.
  - Attack transient risetime measurement: $\Delta t_{10-90} \le 15\text{ ms}$.
  - Octave-band decay rate matching: $RMSE_{T60} \le 0.08$.
  - Multi-Resolution STFT Loss ($L_{MRSL}$) across multi-scale windows $M \in \{512, 1024, 2048, 4096\}$.

---

### Tier 6: Micro-Mechanical Action Noise & Physical Articulations - [COMPLETED]
- [x] **Key-Bottom Thump & Action Escapement Dynamics**:
  - Modeled non-linear contact impact between wooden key levers, balance pins, and keybed felt punchings driving shared spruce modal resonators.
  - Synthesized subtle mechanical click of the escapement jack let-off during soft pianissimo playing and key-up back-rail felt clack.
- [x] **Damper Lift & Restrike Felt Friction**:
  - Modeled momentary broadband "whoosh" sound of 88 dampers simultaneously lifting off strings upon sustain pedal depression.
  - Restrike damping friction: synthesized high-frequency friction buzz/chatter when descending dampers touch vibrating strings upon NoteOff.
- [x] **Pedal Mechanism Physics**:
  - Mechanical squeaks, trapwork lever velocity scaling, and whole cast-iron plate structural frame impulse resonance (58 Hz, 165 Hz, 340 Hz) upon rapid pedal stomp.

---

### Tier 7: Spatial Multi-Microphone Soundboard Radiation & True IR Profiler - [COMPLETED]
- [x] **Multi-Perspective Concurrent UPOLS Convolution Engine**:
  - Zero-allocation multi-perspective partitioned overlap-save convolver with shared forward FFT across all channels (45%+ CPU reduction).
  - Three distinct listening perspectives: **Close** (hammer rail / bright transient), **Player** (binaural HRTF seated perspective), and **Ambient / Room** (Decca tree diffuse hall tail).
- [x] **Continuous Lid Opening Baffle Model**:
  - Acoustic shadowing and high-shelf diffraction filter continuously adjustable from Closed ($0^\circ$), Half-stick ($15^\circ$), Full-stick ($45^\circ$), to Lid Removed ($60^\circ$).
  - Discrete reflection comb/delay network capturing acoustic lid reflections.
- [x] **Calibrated Physical Orthotropic Soundboard Profiling**:
  - Analytical Mindlin-Timoshenko orthotropic spruce plate IR generator calibrated for Close, Player, and Ambient spatial radiation.

---

### Tier 8: Native Hardware-Accelerated GUI & Standalone App - [COMPLETED]
- [x] **Native CLAP GUI (`clap_plugin_gui`)**:
  - Hardware-accelerated 2D interface implemented in Rust (`nih-plug` + `egui 0.31`) with zero runtime GC pauses.
  - Real-time visualization of bridge dual-polarization orbital motion ($u_T$ vs $u_P$ Lissajous curves) and peak VU meters.
  - Interactive 88-key piano keyboard with velocity-sensitive clicking/dragging and active key illumination.
- [x] **Standalone Playable Desktop App (`physics-piano-standalone`)**:
  - Playable desktop application with ALSA, JACK, CoreAudio, and WASAPI audio & MIDI driver support, allowing standalone playing with mouse or MIDI keyboard without a DAW.
- [x] **Physical Parameter & Voicing Rack**:
  - Real-time parameter sliders: sustain pedal (half-pedaling), una corda, inharmonicity scale, hammer hardness, unison detuning, phantom partial gain, and master volume.
- [x] **Automated Multi-Platform Release CI/CD**:
  - GitHub Actions matrix workflow (`.github/workflows/ci.yml`) compiling, testing, bundling `.clap` plugins, and publishing release artifacts across Linux, macOS, and Windows.

---

### Tier 9: Differentiable Physics & Neural-Hybrid Auto-Voicing - [COMPLETED]
- [x] **Differentiable Physical Simulation Loop (`physics_piano.autovoicing`)**:
  - Differentiable forward simulation model parameterized by string tension $T_0$, Young's modulus $E$, hammer stiffness $K_h$, hammer non-linear exponent $p$, and damping parameters $(\sigma_0, \sigma_1)$.
  - Multi-Resolution STFT Spectral Loss ($L_{\mathrm{MRSL}}$) optimizer combining multi-scale spectral convergence and logarithmic magnitude distance across multiple FFT frame sizes (e.g. 512, 1024, 2048).
  - Iterative Nelder-Mead simplex optimizer calibrating physical parameters against real acoustic target recordings (e.g., Steinway Model B samples) and exporting calibrated voicing profiles to JSON (`calibrated_voicing_A4.json`).
- [x] **Physics-Informed Neural Operators & Mindlin-Timoshenko Plate Surrogate (`SoundboardPINOSurrogate`)**:
  - Orthotropic 2D Mindlin-Timoshenko plate surrogate incorporating Sitka spruce grain elasticity tensor ($D_x, D_y, D_{xy}, D_1$), aspect ratio, and modal curvature.
  - Analytic prediction of 2D soundboard resonant modal frequencies $\omega_{mn}$ and bridge driving-point mobility matrix $\mathbf{Y}_{\mathrm{bridge}}(\omega)$ without full-mesh 3D PDE solving.
- [x] **Voicing Calibration Pipeline & Demo**:
  - `examples/auto_voicing_demo.py` calibrating against target audio, yielding measurable loss reduction (8.0% MRSL reduction on Steinway B A4).

---

### Tier 10: Extreme Real-Time Performance & High-Polyphony Architecture
- [x] **Zero-Allocation Hard Real-Time Audio Core - [COMPLETED]**:
  - Eliminated all heap allocations and dynamic `Vec` sizing in the audio process callback.
  - Replaced inner-loop hash table traversals with contiguous slice indexing (`active_keys_vec`).
  - Hoisted radiation mode string comparisons outside audio sample loops.
- [x] **Viscoelastic Damper 2nd-Order Taylor Series Expansion - [COMPLETED]**:
  - Replaced per-sample transcendental `exp()` calls in inner modal loops with high-precision 2nd-order Taylor polynomials ($e^{-x} \approx 1 - x + 0.5x^2$).
  - Eliminated tens of millions of hardware `exp()` instructions per second with $< 10^{-9}$ error.
- [x] **Audio-Block Boundary Voice Lifecycle & Intelligent Voice Stealing - [COMPLETED]**:
  - Moved note termination energy checks from per-sample to block boundary (128x compute reduction).
  - Raised dynamic note retirement threshold to $-70\text{ dB}$ ($1.0 \times 10^{-7}$), allowing inaudible damped notes to retire naturally even with bridge cross-coupling.
  - Upgraded concurrent active voices to `MAX_ACTIVE_VOICES = 32` with lowest-energy released-first voice stealing, preventing CPU overload and DAW buffer underruns (xruns).
- [x] **Single-Pass Fused Modal Stepping & Bridge Force Accumulation - [COMPLETED]**:
  - Combined `string.step()` and `get_bridge_forces()` into a single loop pass, maintaining modal states in CPU registers and halving L1 cache memory reads.
  - Inactive hammer contact fast-path: completely bypassed strike spatial projection during 99.9% of note duration once hammer rebounds.
  - Specialization of undamped sustain loops: eliminated branches and modal damping coefficient memory loads during steady-state sustain.
  - Fast-path for center stereo panning, removing per-sample `sin()` and `cos()` trigonometric evaluations.
- [x] **Register-Adaptive Modal Truncation (Psychoacoustic Nyquist Culling) - [COMPLETED]**:
  - Dynamically scale modal harmonic count based on note fundamental frequency: $N_m = \text{clamp}\left(\lfloor \frac{20000}{f_0} \rfloor, 6, 35\right)$.
  - Truncates supersonic modes exceeding human hearing range ($> 20\text{ kHz}$) in treble registers (e.g., A0 keeps full 35 modes, C7 culls to 9 modes, C8 culls to 6 modes).
  - Slashes treble register compute load by up to 70% with zero perceptible loss of audible bandwidth, freeing up compute budget for 32 concurrent voices.
- [x] **Lock-Free Thread Safety & MIDI CC Isolation - [COMPLETED]**:
  - Lock-free `AtomicU64` bitset for active key state query from GUI, eliminating thread lock contention between audio real-time thread and GUI rendering loop.
  - Edge-triggered MIDI CC vs GUI slider disambiguation, preventing race conditions on sustain / soft pedals.
- [ ] **Explicit SIMD Vectorization & Structure-of-Arrays (SoA) Layout**:
  - Transform modal state storage from Array-of-Structures (AoS: `Vec<ModalState>`) to 32-byte aligned Structure-of-Arrays (SoA: `[f64; 32]`, `[f32; 32]`).
  - Implement explicit AVX2 / AVX-512 FMA (`_mm256_fmadd_pd`) and ARM NEON (`vfma_f64`) inner modal kernels.
- [ ] **Mixed-Precision Computing (`f32` Modal Oscillators + `f64` Geometric Tension Accumulation)**:
  - Transition modal state updates ($q, v, \Phi, \Gamma$) to 32-bit single precision (`f32`), halving memory bandwidth and doubling vector lane throughput.
  - Preserve 64-bit double precision (`f64`) strictly for geometric non-linear string tension accumulation $\Delta T(t)$ and bridge reaction feedback to eliminate long-term DC drift.
- [ ] **Multi-Core Voice Parallelism (Host Collaborative Thread Pool & Rayon Work-Stealing)**:
  - Decouple inter-string bridge reaction force across active voices with 1-sample delay.
  - Scale seamless real-time polyphony to 64+ concurrent voices for extreme virtuoso piano literature.
- [ ] **Vectorized UPOLS Partitioned FFT Convolver**:
  - Accelerate zero-latency partitioned impulse soundboard convolution using explicit AVX2/NEON complex vector multiply-accumulate and optimized FFT backends.

---

### Tier G: Deep Acoustic Realism Engine for Guitar (`crates/physics-guitar`) - [COMPLETED]
- [x] **P0 Dual-Polarization Bridge Dynamics**: Horizontal shear coupled with ~18% bridge rocking, low in-plane radiation loss delivering authentic two-stage acoustic decay (Prompt & Aftersound).
- [x] **P1 All-String Sympathetic Resonance**: Bridge velocity feedback driving open resting strings across the soundboard and body.
- [x] **P2 Nonlinear Tension Modulation (Kirchhoff-Carrier Strain)**: Transverse stretch restoring force delivering attack twang and dynamic pitch settling (Pitch Glide).
- [x] **P3 Adaptive Pluck & Tactile Snap**: String fundamental-dependent release impedance and celluloid pick tactile snap impulse.
- [x] **P4 Stereo Acoustic Radiation & Body Cavity**: Christensen soundhole center radiation, upper/lower bout binaural radiation, and dual allpass wood dispersion.
- [x] **P5 6 Factory Presets & egui Real-Time Fretboard Interface**: Martin D-28, Dreadnought Strummer, Strat Clean Chime, Texas Blues Breakup, Les Paul Warm Jazz, Flamenco Con Brio.

---

## License

This project is licensed under the MIT License or Apache-2.0 License at your option.
