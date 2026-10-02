# Physics Bass Roadmap

This roadmap tracks the physical bass engine, playable editor, plugin integration, and future acoustic/electric bass research milestones (`crates/physics-bass`).

---

## Completed Milestones

### Milestone 1: Physical Bass Engine — COMPLETED
- [x] CFL-bounded 1D FDTD stiff-string model with Kirchhoff-Carrier geometric tension modulation.
- [x] Bounded unilateral fret contact dynamics (low-rank barrier solve).
- [x] Finger, pick, and slap excitation models with low B0 string support.
- [x] Finite Gaussian aperture magnetic pickup, air-gap quadratic distortion, and RLC tone circuit.
- [x] Wooden body reciprocal coupling with A0/B1-/B1+/Bridge Hill signature modes.

### Milestone 2: Playable Editor and Plugin — COMPLETED
- [x] Vizia editor with interactive 5-string, 24-fret fretboard and MIDI auditioning.
- [x] Physical parameter racks and preset selection.
- [x] CLAP, VST3, and standalone plugin adapter.

### Milestone 3: 3/4 Upright Acoustic Bass Modeling & GUI Velocity Upgrade — COMPLETED
- [x] **Dedicated 3/4 Double Bass String Models (`acoustic_four` / `acoustic_five`)**:
  - Authentic 1.05m (~41.3") scale length.
  - Low Young's modulus flatwound/gut core ($1.1 \times 10^{11}$ Pa), heavier linear mass density, and higher internal friction damping.
  - Exact open tuning preservation ($< 2 \times 10^{-5}$ frequency error).
- [x] **Real-Time FDTD Parameter Reconfiguration**: Dynamic zero-allocation string reconfiguration on `BassMode::Acoustic` switch.
- [x] **Acoustic Double f-Hole Spatial Expansion**: Dynamic stereo field widening reflecting double f-hole radiation.
- [x] **Interactive Dynamic Strike Velocity & QWERTY Audition**: Radial/vertical velocity weighting on fretboard and computer keyboard playing.

---

## Future Milestones

### Milestone 4: Contact Mechanics, Legato & Slap/Ghost Articulations (接触动力学与演奏法)
- [ ] **Unconditional Energy-Stable Contact Solver (SAV / IEQ Formulation)**:
  - Upgrade penalty-based fret contact to a Scalar Auxiliary Variable (SAV) / Invariant Energy Quadratization (IEQ) discrete formulation.
  - Guarantee strictly non-increasing discrete energy ($\Delta E \le 0$) under extreme slap velocities and low string action.
- [x] **FDTD Physical Legato Engine (Hammer-on & Pull-off)**:
  - Transition between notes on the vibrating FDTD spatial grid without clearing wave states.
  - Spatial grid resampling and coordinate projection preserving stored string vibrational energy.
  - Adjust effective stopping boundary dynamically and inject localized fret impact transients.
- [x] **Slap, Pop & Ghost Notes (击勾弦与死音/切音)**:
  - *Slap Thumb Snap*: Nonlinear shock-wave steepening under high hammer strike velocity.
  - *Pop Pull-and-Release*: Transverse hook displacement release with high snap attack and fret collision.
  - *Ghost / Dead Notes*: Ultra-short excitation with Kelvin-Voigt viscoelastic flesh damping ($T_{60} \approx 18$ ms) for authentic Funk/R&B rhythmic chucks.
- [ ] **Neck Relief & Fret Crown Curvature**:
  - Incorporate realistic neck relief parabolic curvature and rounded fret crown profiles.

### Milestone 5: Bowed String Physics & Acoustic Resonance Details (低音提琴拉奏与声学细节)
- [ ] **Double Bass Bowed String Physics (Arco 弓弦摩擦动力学)**:
  - Implement a continuous stick-slip friction solver using the nonlinear Coulomb-Stribeck curve:
    $$\mu(v_{\text{rel}}) = \mu_k + (\mu_s - \mu_k) e^{-(v_{\text{rel}} / v_0)^2}$$
  - Expose Bow Velocity, Bow Pressure, and Bow Contact Position controls for classical/jazz orchestral upright bass.
- [ ] **Neck Admittance & Dead Spots (琴颈吸收与死音点)**:
  - Couple a 2nd-order neck bending resonator at the nut/headstock boundary.
  - Reproduce the iconic Fender G-string 5th~7th fret (C#3/D3) dead spot attenuation.
- [x] **Dual J-Bass Geometry & Continuous Pickup Blend**:
  - Variable neck/bridge pickup continuous blend with phase-cancellation mid-scoop for Jazz Bass slap & finger funk tones.
- [ ] **Split-Coil P-Bass Magnetic Geometry**:
  - Physical offset split-coil pickup configuration with independent pole pieces for Precision Bass.

### Milestone 6: GUI Visuals, SIMD Performance & Architecture (可视化与高性能计算)
- [ ] **60 FPS Real-Time FDTD Waveform Visualizer**:
  - Direct 60 FPS Skia rendering of the 256-point string displacement grid on the GUI, showing wave travel, reflections, and fret collisions in real time.
- [ ] **Explicit SIMD Vectorization (AVX2 / ARM NEON)**:
  - Explicit SIMD vectorization for `slope_integral` and the 5-point stiff-string finite difference kernel, reducing FDTD compute time by 40%~60%.
- [ ] **Bass Amp & Preamp Head GUI Rack**:
  - Dedicated visual control rack for `Drive`, `Tone`, `Fret Buzz`, `Body Mix`, and `Pluck Style` (Finger/Pick/Slap/Arco).
- [ ] **Dynamic Silence Culling**:
  - Sleep mode for inactive voices below energy threshold to conserve CPU on polyphonic passages.
