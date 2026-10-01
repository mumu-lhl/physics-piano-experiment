# Physical Bass and Drum Implementation Status

This document records what the current Bass and Drum crates actually simulate. “Implemented” describes the shipped finite-dimensional real-time models; it does not imply calibration against a particular instrument or a formal stability proof for every parameter combination.

## Bass (`physics-bass`)

- Fixed-array FDTD string states with frequency-dependent stiffness, tuning, pitch bend, and pluck/pick/slap excitation.
- Fractional fret contact at up to four simultaneously active frets. Contact uses a fixed-size low-rank SAV/IEQ-style auxiliary-potential update, solved with a bounded Cholesky/Woodbury system; contact locations are interpolated between grid nodes.
- Pickup aperture sampling and a parameterized passive RLC-inspired pickup/tone network.
- Modal body response feeds bridge displacement back into the string boundary.
- String routing, parameter smoothing, presets, MIDI/editor/CLI paths are integrated.

## Drum kit (`physics-drum`)

- Kick, snare, and tom heads use spatially excited circular-membrane Bessel modes, with fixed modal arrays, nonlinear restoring terms, and point-force/pressure coupling.
- Kick/snare double heads exchange force through a compliant, damped air-cavity state.
- The snare has 24 distributed wire/contact states; modal spatial shapes are precomputed so Bessel functions are not evaluated in the sample loop.
- Cymbals use 96 damped modes sampled across the report's inharmonic frequency anchors, with nonlinear nearest-neighbor modal coupling, instrument-specific spectra, and separate modal/noise decay; open/closed hi-hat decay and choke behavior respond to MIDI controls.
- Voice triggering, decay controls, parameter smoothing, presets, MIDI/editor/CLI paths are integrated.

## Real-time and regression checks

- The Bass and Drum audio paths use fixed-size state and bounded work. Regression tests exercise active-note rendering under an allocation detector.
- `cargo test -p physics-bass -p physics-drum`, `cargo check -p physics-bass -p physics-drum --all-targets`, and targeted Clippy checks pass in the current development environment.
- Offline CLI renders were checked for finite output. This is a stability/smoke check, not a spectral match claim.

## Remaining validation and fidelity work

- Bass pickup/body values still need fitting against measured pickup, bridge, and body frequency-response data; the current passive network and body are compact approximations.
- Bass fret contact is a bounded low-rank SAV/IEQ-style discretization, not a formal proof of unconditional stability over all extreme settings or an unlimited-contact solver.
- Drum heads are reduced to 32 circular modes rather than a full 2-D membrane mesh. Cymbal vibration is a 96-mode reduced model, not a full plate mesh or full von Kármán PDE solver.
- The drum cavity, snare-wire contacts, mallet contact, and acoustic radiation remain reduced-order approximations; full spatial room/radiation coupling is not implemented.
- No measured-instrument FRF/STFT regression corpus or blind listening evaluation exists yet. Do not report physical-accuracy percentages until that calibration and evaluation pipeline is built.
