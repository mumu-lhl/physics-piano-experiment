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

---

## Future Milestones

### Tier G4: Transducers, Feedback Loops & Low-Latency IRs (换能器、自激反馈与卷积)
- [ ] **Acoustic Feedback / Larsen Effect Closed Loop (电吉他音箱声学自激反馈)**:
  - Establish a real-time feedback loop from the amplifier/cabinet output back into string modal accelerations through an adjustable air-propagation delay line (2~8 ms).
  - Reproduce authentic singing feedback sustain and harmonic overtone transitions.
- [ ] **Stratocaster In-Between Quack & Blend Network**:
  - Reverse-phase pickup wiring and continuous neck/middle/bridge blend impedance, modeling pickup phase cancellation.
- [ ] **Low-Latency Partitioned Convolution IR Engine (UPOLS)**:
  - Ultra-low latency partitioned convolution engine (5~10 ms block size) for blending measured soundboard high-frequency residual IRs and custom electric cabinet mic IRs.

### Tier G5: GUI Visuals, Stompbox Controls & SIMD Optimization (界面可视化与工程性能)
- [ ] **60 FPS Real-Time String Vibration Waveform Visualizer**:
  - Sample modal displacements along the string to render slow-motion traveling wave packets and standing waves on the GUI fretboard.
- [ ] **Vintage Amp Head & Stompbox UI Rack**:
  - Top-mounted collapsible control strip featuring rotary knobs for `Drive`, `Bass`, `Middle`, `Treble`, `Presence`, `SAG`, and `Pickup Selector`.
- [ ] **Multi-Rate Oversampling & Anti-Aliasing (2x/4x)**:
  - Minimum-phase polyphase IIR oversampling for high-gain preamp distortion stages to eliminate Nyquist foldback aliasing.
- [ ] **Dynamic Energy Culling & Silence Sleep**:
  - Automatically bypass modal integration when string energy decays below -96 dBFS, saving CPU during sparse passages.
