# Physics Piano Experiment (WIP)

A first-principles, formula-driven physical modeling acoustic piano synthesizer in Python.

This project implements a complete continuous mechanical simulation of an acoustic grand piano, translating Euler-Bernoulli wave mechanics, Hunt-Crossley felt impact dynamics, anisotropic bridge admittance, and soundboard radiation into a high-performance discrete state-space synthesis engine.

---

## Key Features

1. **Euler-Bernoulli Damped Stiff String Mechanics**:
   - Continuous governing PDE:
     $$\rho A \frac{\partial^2 u}{\partial t^2} = T_0 \frac{\partial^2 u}{\partial x^2} - E I \frac{\partial^4 u}{\partial x^4} - 2\rho A \sigma_0 \frac{\partial u}{\partial t} + 2\rho A \sigma_1 \frac{\partial^3 u}{\partial t \partial x^2} + F_h(x, t)$$
   - Analytical modal angular frequencies with exact inharmonicity $B$:
     $$\omega_n = n \omega_0 \sqrt{1 + B n^2}, \quad B = \frac{\pi^3 E r^4}{4 T_0 L^2}$$
   - Exact continuous-to-discrete matrix exponential state-space transitions ($\Phi, \Gamma$) for unconditional stability and zero numerical frequency warping.

2. **Dual-Polarization & Two-Stage Decay (Prompt Sound & Aftersound)**:
   - Each string features independent vertical ($u_T$) and horizontal ($u_P$) transverse polarizations.
   - Anisotropic bridge driving-point mobility matrix:
     $$\begin{bmatrix} V_T(\omega) \\ V_P(\omega) \end{bmatrix} = \begin{bmatrix} Y_{TT} & Y_{TP} \\ Y_{PT} & Y_{PP} \end{bmatrix} \begin{bmatrix} F_T(\omega) \\ F_P(\omega) \end{bmatrix}$$
   - Energy transfers from vertical modes ($\text{Re}(Y_{TT}) \gg \text{Re}(Y_{PP})$) to low-loss horizontal modes, yielding the characteristic biexponential decay.

3. **Hunt-Crossley Nonlinear Felt Hammer Contact**:
   - Compressive hysteretic contact force:
     $$F_h(t) = \max\left(0, K_h [\eta(t)]^p + \lambda_h [\eta(t)]^p \frac{d\eta(t)}{dt}\right)$$
   - Realizes velocity-dependent contact duration contraction without unphysical adhesive suction forces upon rebound.

4. **Unison String Triplet & Micro-Detuning**:
   - Trichords (3 unison strings per note) with sub-cent mistuning (e.g. $\pm 0.38$ cents), generating natural acoustic beating, chorus richness, and delayed radiation.

5. **Damper Network & Sympathetic Resonance**:
   - Continuous viscoelastic damper network.
   - When the sustain pedal is depressed, bridge boundary vibration dynamically couples back into all undamped strings across the entire keyboard, producing full sympathetic harmonic resonance.

6. **Objective Acoustic Verification Suite**:
   - Automated non-linear least-squares regression for inharmonicity $B$ ($\epsilon_B \le 1.2\%$).
   - Schroeder backward integration ($EDC$) for two-stage decay ratio $\tau_{after} / \tau_{prompt} \in [3.5, 8.0]$.
   - Hammer contact duration ladder measurement verifying monotonic contraction.

---

## Installation

```bash
# Clone the repository
git clone https://github.com/mumu-lhl/physics-piano-experiment.git
cd physics-piano-experiment

# Install in editable mode
pip install -e .
```

### Dependencies
- Python >= 3.9
- `numpy >= 1.22.0`
- `scipy >= 1.8.0`

---

## Python API Usage

```python
from physics_piano.api import PianoSynth

# 1. Initialize synthesizer (48 kHz, 35 modes per string)
synth = PianoSynth(sample_rate=48000, num_modes=35)

# 2. Render a single note (e.g., A4, 440 Hz)
audio_note = synth.render_note(
    pitch="A4",
    velocity=0.8,
    duration=3.0,
    sustain=False
)
synth.export_wav("output_a4.wav", audio_note)

# 3. Render a polyphonic chord with sustain pedal & sympathetic resonance
audio_chord = synth.render_chord(
    pitches=["C4", "E4", "G4"],
    velocity=0.85,
    duration=4.0,
    sustain=True
)
synth.export_wav("output_chord.wav", audio_chord)
```

---

## Command Line Interface (CLI)

The package provides a unified CLI tool: `physics-piano` (or via `python -m physics_piano.cli`).

### 1. Audio Synthesis
```bash
# Synthesize single note C4
physics-piano render --note C4 --velocity 0.8 --duration 3.0 -o c4.wav

# Synthesize chord with sustain pedal
physics-piano render --chord "C4,E4,G4" --velocity 0.85 --duration 4.0 --sustain -o chord.wav
```

### 2. Objective Acoustic Metric Verification
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

### 3. Performance Benchmark
```bash
physics-piano benchmark --modes 30
```

---

## Running Tests

```bash
python -m unittest discover tests
```

---

## Comprehensive Roadmap (WIP to Production Full Version)

The current Python implementation serves as a functional, first-principles proof-of-concept and physical validation harness. The following multi-tier roadmap outlines the architectural path toward an industrial-grade, real-time physical modeling virtual instrument.

### Tier 1: Current Experimental Prototype (WIP Python Core) - [COMPLETED]
- [x] **Modal State-Space Stiff String Engine**: Euler-Bernoulli fourth-order dispersion equation solved via exact continuous-to-discrete matrix exponential transitions (Phi, Gamma).
- [x] **Hunt-Crossley Nonlinear Felt Impact**: Hysteretic compression model F_h(t) = max(0, K_h * [eta]^p + lambda_h * [eta]^p * d(eta)/dt) with velocity-dependent contact duration contraction.
- [x] **Bridge Anisotropic Mobility**: Cross-polarization coupling (Y_TP != 0) inducing dual-polarization energy transfer and biexponential decay (Prompt sound vs. Aftersound).
- [x] **Unison Trichord Dynamics**: 3-string unison groups with micro-detuning interference and beating.
- [x] **Damper Network & Sympathetic Bus**: Viscoelastic damping and global bridge-driven sympathetic resonance under sustain pedal.
- [x] **88-Key Parameter Generator**: Continuous parameter scaling across full concert grand keyboard (A0 to C8).
- [x] **Unified CLI & API**: Note/chord rendering and automated objective acoustic inspection (`physics-piano render/verify/benchmark`).
- [x] **Objective Metric Test Suite**: Automated verification for inharmonicity regression (epsilon_B <= 1.2%, sigma(Delta C) < 2.0 cents) and Schroeder EDC decay ratio.

---

### Tier 2: Advanced Continuous Continuum Mechanics & Nonlinearities
- [ ] **Large-Amplitude Geometric Nonlinearity (Phantom Partials)**:
  - Formulate string geometric stretching strain: epsilon(t) = (1 / 2L) * int_0^L (du/dx)^2 dx.
  - Implement time-varying longitudinal tension modulation: T(t) = T_0 + (EA / 2L) * int_0^L (du/dx)^2 dx.
  - Couple longitudinal displacement waves with the bridge admittance component Y_TL, reproducing the characteristic metallic "zing/phantom partials" during fortissimo bass strikes.
- [ ] **Orthotropic 2D Reissner-Mindlin Soundboard Continuous Simulation**:
  - Transition from lumped multi-modal IIR resonators to a measured multi-point driving-point mobility matrix Y_bridge(omega) across the long and short bridges.
  - Implement Partitioned Uniform-Power Overlap-Save (UPOLS) FFT block convolution for real-time, zero-latency soundboard radiation rendering.
  - Model rib stiffeners, soundboard crown geometry, and wood grain anisotropy.
- [ ] **Fully Implicit Energy-Preserving Bridge Coupling (SAV Scheme)**:
  - Replace local reaction approximations with a Scalar Auxiliary Variable (SAV) / Energy Quadratisation discrete scheme to couple strings, bridge, and soundboard in a strictly dissipative, unconditionally stable algebraic loop.
- [ ] **Discontinuous 88-Key Scale Calibration**:
  - Incorporate empirical discontinuous break points: single-wound copper strings (A0-E1), double-wound strings (F1-Bb2), and plain steel wire transition (B2-C8).
  - Benchmark against physical laser vibrometry measurements from concert grands (Steinway D-274 / Yamaha CFX).

---

### Tier 3: High-Performance Real-Time Engine (C++20 / Rust)
- [ ] **Core DSP Engine Migration**:
  - Rewrite core modal update loops and contact solvers in modern C++20 or Rust.
  - Zero-heap allocation during realtime audio processing loop (hard real-time process callback guarantee).
  - Explicit 64-byte cache-line alignment to eliminate inter-core false sharing.
- [ ] **SIMD Vectorization**:
  - Implement vectorized parallel modal biquad / state-space updates using AVX2, AVX-512, and ARM Neon intrinsics.
  - Process unison string groups and multi-mode banks in single-instruction wide registers.
- [ ] **Voice Lifecycle & Energy-Driven Garbage Collection**:
  - Implement note energy monitors (E_modal < -96 dB) to dynamically release quiet voices and dispatch CLAP_EVENT_NOTE_END events.

---

### Tier 4: Native CLAP Plugin Architecture
- [ ] **Host Collaborative Thread Pool Integration (clap_host_thread_pool)**:
  - Implement `clap_plugin_thread_pool` interface to dispatch polyphonic string-block workloads to DAW host-provided real-time threads.
  - Eliminate internal `std::thread` overhead, lock contention, and OS priority inversion, achieving stable operation under 64-sample audio buffer budgets.
- [ ] **Sample-Accurate Modulation & Note Expressions**:
  - Sample-accurate velocity translation: v0 = v_max * (velocity)^gamma_felt with microsecond timestamp offsets.
  - Per-note modulation (CLAP_EVENT_NOTE_EXPRESSION):
    - `CLAP_NOTE_EXPRESSION_TUNING`: Dynamic fundamental tension T0 modulation for historical temperaments, microtonal tuning, and dynamic stretch tuning.
    - Continuous damper pedal articulation (half-pedaling and soft pedal Una Corda shifting hammer strike location x_h).
- [ ] **Standard Plugin Formats Packaging**:
  - Provide native CLAP binary distribution with optional VST3/AU wrappers via CPLUG or NIH-plug.

---

### Tier 5: Automated Non-Subjective CI/CD Acoustic Regression Pipeline
- [ ] **Reference Dataset Integration**:
  - Ingest Paris Sorbonne MAPS dataset (Disklavier MIDI-aligned acoustic grand recordings) and VSL near-field dry recordings.
  - Automate Dynamic Time Warping (DTW) sample-level onset alignment against synthetic renders.
- [ ] **Perceptual & Multi-Resolution Spectral Loss**:
  - Multi-Resolution STFT Loss (L_MRSL) across multi-scale analysis windows M in {512, 1024, 2048, 4096}.
  - Integrate ITU-R BS.1387 (PEAQ) acoustic model to generate Objective Difference Grade (ODG), enforcing a baseline threshold ODG >= -1.2.
- [ ] **Headless GitHub Actions CI/CD Pipeline**:
  - Automated compilation, rendering of benchmark MIDI test suites, calculation of physical acoustic residuals, and regression graph generation on every pull request.
