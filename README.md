# Physics Piano, Guitar & Bass Synthesizers

A collection of piano, guitar, and bass virtual instruments built in **Rust**, with native CLAP / VST3 plugin bundles and standalone desktop applications. The repository also includes an offline Python validation, calibration, and differentiable physics harness. The experimental physical-modeling drum instrument is archived and is not part of releases.

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
│   │   ├── src/gui/              # Vizia interface & 88-key keyboard
│   │   ├── src/params/           # 88-key grand piano physical parameter generator
│   │   ├── src/clap_plugin.rs    # Low-level CLAP C-ABI bindings
│   │   └── src/nice_plugin.rs    # nice-plug adapter (CLAP, VST3, Standalone)
│   └── physics-guitar/           # Acoustic & Electric Guitar physical modeling engine
│       ├── src/bin/              # Guitar standalone playable app & CLI
│       ├── src/core/             # Guitar string, tension modulation, body radiation, pluck
│       ├── src/gui/              # Real-time fretboard tracking & string energy visualizer
│       ├── src/presets.rs        # 6 factory presets (Martin D-28, Strat, Les Paul, etc.)
│       └── src/nih_plugin.rs     # Native CLAP / VST3 / Standalone guitar plugin
│   ├── physics-bass/             # FDTD electric/acoustic bass engine and plugin adapter
│   │   ├── src/string.rs         # CFL-bounded stiff-string grid, fret contact, slap
│   │   ├── src/acoustic.rs       # Finite pickup aperture and bass-body modes
│   │   └── src/nice_plugin.rs    # CLAP / VST3 / standalone adapter
│   └── physics-drum/             # Archived physical-modeling drum experiment
│       ├── src/membrane.rs       # Bessel-ratio circular membrane modes
│       ├── src/voices.rs         # Double-head, snare-wire and cymbal models
│       └── src/nice_plugin.rs    # General-MIDI CLAP / VST3 adapter
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
  - Vizia interface featuring real-time fretboard tracking, active fret indicators, string energy meters, and parameter racks.

### 3. Physical Bass Engine (`crates/physics-bass`)
* **CFL-bounded stiff-string FDTD** with geometric tension, bounded fret contact, and finger/pick/slap excitation.
* **Electric and acoustic paths**: finite magnetic pickup aperture, tone filtering, and A0/B1/Bridge-Hill body modes.
* **Playable Vizia editor**: interactive 5-string/24-fret fretboard, atomic vibration display, MIDI audition events, and physical parameter racks.

### 4. Archived Drum Experiment (`crates/physics-drum`)
The physical-modeling drum instrument is archived because its timbres did not
match recorded and sampled drum kits well enough. Its source and reference-based
evaluation remain available for research, but GitHub Actions does not test or
release it. The planned replacement is a sample-based engine; see the
[drum roadmap](crates/physics-drum/ROADMAP.md).

---

## Quickstart: Building & Running

### Prerequisites
- **Rust Toolchain**: `stable` (MSRV 1.85+, Rust 2024 Edition)
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

# Bass: render low E1 (electric; use "acoustic" as the final argument for body radiation)
cargo run --release -p physics-bass --bin physics-bass-rs -- E1 3.0 bass_e1.wav 0.85
```

### 4. Running Workspace Test Suite
Verify the active piano, guitar, and bass crates:

```bash
cargo test --workspace --exclude physics-drum
```

The archived drum timbre evaluation is separate and needs downloaded reference
audio (stored under the ignored `target/` directory):

```bash
python3 tools/download_drum_references.py
cargo nextest run -p physics-drum --test timbre_eval -- --nocapture
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

The native Rust engine also has audio-output regressions. These check rendered partials against the stiff-string inharmonicity model, velocity-dependent attack brightness, unison detuning spread, damper release, and hammer contact contraction:

```bash
cargo test -p physics-piano --test test_acoustic
```

These tests validate physical relationships without licensed reference recordings. MAPS/VSL comparison remains an optional listening-reference audit.

The bass engine has a matching no-reference acoustic regression suite for open-string pitch, stiff-string inharmonicity, dynamic level, pickup-position response, acoustic-body resonance, and note release. The bass pickup's passive tone-resonance shift also has a unit regression test:

```bash
cargo test -p physics-bass
```

These checks verify rendered physical relationships; comparison against measured bass recordings remains a separate calibration step.

---

## Continuous Integration & Release (GitHub Actions)

The project includes an automated matrix CI/CD pipeline (`.github/workflows/ci.yml`):
- **Cross-Platform Matrix**: Automated compile and test passes for Piano, Guitar, and Bass on Linux (`ubuntu-latest`), macOS (`macos-latest`), and Windows (`windows-latest`) on every push and PR. The archived drum crate is excluded.
- **CLAP & VST3 Plugin Bundles**: Builds and packages Piano, Guitar, and Bass plugins plus standalone apps across Linux, macOS, and Windows. Drum releases are paused while the sample-based replacement is developed.
- **Automated Releases**: Pushing a version tag (`git tag v0.1.0 && git push origin v0.1.0`) triggers a GitHub Release with multi-platform `.clap` archive downloads.

---

## Instrument Roadmaps

Each instrument's milestones are maintained beside its crate:

- [Piano roadmap](crates/physics-piano/ROADMAP.md)
- [Guitar roadmap](crates/physics-guitar/ROADMAP.md)
- [Bass roadmap](crates/physics-bass/ROADMAP.md)
- [Drum roadmap](crates/physics-drum/ROADMAP.md)

## License

This project is licensed under the MIT License or Apache-2.0 License at your option.
