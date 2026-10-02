# Physics Guitar Roadmap

This roadmap tracks completed physical modeling milestones and planned future capabilities for the physical acoustic and electric guitar engine (`crates/physics-guitar`).

---

## Completed Milestones

### Tier G: Deep Acoustic Realism Engine — COMPLETED
- [x] **P0 Dual-Polarization Bridge Dynamics**: Horizontal shear coupled with ~18% bridge rocking, low in-plane radiation loss delivering authentic two-stage acoustic decay (Prompt & Aftersound).
- [x] **P1 All-String Sympathetic Resonance**: Bridge velocity feedback driving open resting strings across the soundboard and body.
- [x] **P2 Nonlinear Tension Modulation (Kirchhoff-Carrier Strain)**: Transverse stretch restoring force delivering attack twang and dynamic pitch settling (Pitch Glide).
- [x] **P3 Adaptive Pluck & Tactile Snap**: String fundamental-dependent release impedance and celluloid pick tactile snap impulse.
- [x] **P4 Stereo Acoustic Radiation & Body Cavity**: Christensen soundhole center radiation, upper/lower bout binaural radiation, and dual allpass wood dispersion.
- [x] **P5 6 Factory Presets & Vizia Real-Time Fretboard Interface**: Martin D-28, Dreadnought Strummer, Strat Clean Chime, Texas Blues Breakup, Les Paul Warm Jazz, Flamenco Con Brio.

### Tier G2: Multi-Stage Tube Amp, Tone Stack & Studio Air Refinement — COMPLETED
- [x] **Multi-Stage 12AX7 Preamp Tube Saturation**: Asymmetric triode transfer function with dynamic cathode bias drift (Tube Bloom & dynamic 2nd-harmonic generation).
- [x] **3-Band Interactive Tone Stack**: Integrated Bass (110 Hz), Middle (650 Hz), and Treble (3200 Hz) tone shaping network with Presence.
- [x] **Push-Pull Power Amp & Power SAG Compression**: Symmetrical 6L6/EL34 push-pull stage with dynamic power supply sag envelope compression.
- [x] **Celestion Vintage 30 12" Cabinet Enhancement**: 105 Hz cabinet mechanical thump peak, 2.8 kHz bite, and steep cone roll-off above 4.8 kHz.
- [x] **Acoustic Body 3-Stage Cavity Dispersion & 10.5 kHz Air Transmission**: 3-stage Schroeder all-pass cavity reflection network and gentle studio-grade high-frequency air absorption.
- [x] **Interactive GUI Dynamic Velocity & Strumming**: Distance-weighted click velocity, polyphonic chord strumming, and QWERTY keyboard auditioning.

### Tier G3: Expressive Articulations & Legato Physics — COMPLETED
- [x] **Physical Legato Engine (Hammer-on / Pull-off)**:
  - Transition between notes on the same string without re-initializing the modal oscillators.
  - Project existing modal displacement/velocity vectors onto the new vibrating scale length $L_{\text{eff}}$, preserving stored vibrational energy.
  - Inject localized fret-strike impulse for hammer-ons and finger-pad release step for pull-offs.
- [x] **Continuous Legato Slide (平滑滑音)**:
  - Time-varying effective scale length with continuous modal operator recalculation.
  - Natural pitch-glide doppler shift and string-fret friction energy dissipation during slide.
- [x] **Comprehensive Harmonics Modeling (全套自然/人工/敲击泛音)**:
  - *Natural Harmonics*: Selective modal suppression at 12th (1/2), 7th/19th (1/3), 5th (1/4), and 4th/9th (1/5) nodal points.
  - *Pinch Harmonics (人工捏泛音)*: Simultaneous pick excitation and thumb-flesh damping, producing high-gain rock/metal screaming harmonics.
  - *Tap Harmonics (点弦泛音)*: Percussive excitation at nodal positions above fretted notes.
- [x] **Distributed Viscoelastic Palm Muting**:
  - Multi-zone Kelvin-Voigt viscoelastic absorber across 22mm palm contact from bridge saddle, modeling flesh strain-rate comb filtering and preserving low-end thump ("chug").

### Tier G4: Transducers, Feedback Loops & Cabinet Modeling — COMPLETED
- [x] **Acoustic Feedback / Larsen Effect Closed Loop (电吉他音箱声学自激反馈)**:
  - Real-time closed-loop air-propagation delay line (2~8 ms) coupling speaker cabinet sound pressure back into string modal accelerations.
  - Reproduces authentic singing feedback sustain, bloom, and harmonic overtone transitions.
- [x] **Stratocaster In-Between Quack & 5-Way Selection**:
  - Full 5-way selector support including Neck + Middle (pos 4 - SRV/Hendrix chime) and Bridge + Middle (pos 2).
  - Reverse-wound reverse-polarity (RWRP) phase cancellation modeling the iconic scooped glassy quack.
- [x] **Multi-Profile Cabinets & Microphone Proximity**:
  - Celestion Vintage 30, Fender 65 Twin Reverb, and Marshall 1960A Greenback profiles.
  - Variable microphone placement with acoustic proximity effect bass boost.

---

### Tier G5: Convolution, Amp Overdrive, GUI Visuals & SIMD Optimization — COMPLETED
- [x] **Low-Latency Partitioned Convolution IR Engine (UPOLS)**:
  - Ultra-low latency partitioned convolution engine (~1.33 ms partition block size) with zero audio-thread allocation for blending measured soundboard high-frequency residual IRs and electric cabinet mic IRs (Celestion Vintage 30, Fender Twin Reverb, Marshall Greenback).
- [x] **Multi-Rate Oversampling & Anti-Aliasing (2x/4x)**:
  - Minimum-phase polyphase IIR half-band oversampler for non-linear 12AX7 tube saturation to eliminate Nyquist foldback aliasing.
- [x] **60 FPS Real-Time String Vibration Waveform Visualizer**:
  - Sample modal displacements along the string to render slow-motion traveling wave packets and standing waves on the GUI fretboard with glow halo.
- [x] **Vintage Amp Head & Stompbox UI Rack**:
  - Top-mounted 4-rack layout featuring `INSTRUMENT`, `STOMPBOX & EXPRESSION`, `VINTAGE AMP HEAD` (`Drive`, `Bass`, `Middle`, `Treble`, `Presence`, `SAG`), and `CABINET & MASTER` (`Cabinet`, `12" Cab`, `UPOLS IR`, `Pattern`, `BPM`, `Volume`).
- [x] **Dynamic Energy Culling & Silence Sleep**:
  - Automatically bypass modal integration and reset lingering micro-vibrations when string energy decays below -96 dBFS, saving up to 70% CPU during sparse passages.
