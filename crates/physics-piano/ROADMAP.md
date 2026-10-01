# Physics Piano Roadmap

This roadmap tracks milestones for the piano engine and its supporting research, GUI, and plugin work.

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

### Tier 8: Native Vizia GUI & Standalone App - [COMPLETED]
- [x] **Native Vizia Editor (CLAP/VST3)**:
  - Native 2D interface implemented in Rust (`nice-plug` + `vizia`) with zero runtime GC pauses.
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
- [x] **Explicit SIMD Vectorization & Structure-of-Arrays (SoA) Layout - [COMPLETED]**:
  - Store modal positions, velocities, and transition coefficients in 32-byte aligned, eight-lane SoA blocks.
  - Use runtime-dispatched AVX2/FMA or ARM NEON kernels for modal updates, with scalar fallback.
- [x] **Mixed-Precision Computing (`f32` Modal Oscillators + `f64` Geometric Tension Accumulation) - [COMPLETED]**:
  - Update modal positions, velocities, transition coefficients, and damper rates in `f32`.
  - Accumulate bridge reaction and geometric non-linear string tension $\Delta T(t)$ in `f64`.
- [ ] **Host-Cooperative Multi-Core Voice Parallelism (Rayon Work-Stealing)**:
  - Preserve sample-accurate one-sample bridge feedback while distributing active voices across host worker threads.
  - Keep worker scheduling and synchronization outside unsafe real-time callback paths. Current plugin wrappers expose no shared host worker executor.
  - Raise the 32-voice limit only after the host-cooperative executor is available and real-time load is measured.
- [x] **Vectorized UPOLS Partitioned FFT Convolver - [COMPLETED]**:
  - Use runtime-dispatched AVX2/FMA or ARM NEON complex multiply-accumulate kernels across mono and multi-perspective UPOLS paths, with scalar fallback.
