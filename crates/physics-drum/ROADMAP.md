# Physics Drum: archived physical model

Status: archived on 2026-10-01. The physical-modeling drum plug-in is retained
in this repository for reference, but it is no longer a release product. The
generic membrane and cymbal models did not produce convincing drum-kit timbres
despite iterative tuning against recorded and sampled kits.

The drum crate is excluded from the default workspace test command in CI and
from all GitHub Actions release bundles. The source and its reference-based
timbre evaluation remain available for local research. Reference audio is
downloaded into the ignored `target/` directory with
`tools/download_drum_references.py`; no recordings are stored in the repository.

## Replacement direction

Replace the physical voices with a sample-based engine. The next implementation
should prioritize:

- velocity-layered, round-robin one-shots for each MIDI drum instrument;
- pitch-preserving playback and interpolation at the host sample rate;
- envelope, cymbal-tail and hi-hat choke controls that do not allocate on the
  audio thread;
- selectable recorded kits, with sample installation handled separately from
  source control and plug-in binaries.

Do not resume physical-model development unless a specific component has a
measurable reason to use it alongside samples.
