# Drum timbre reference evaluation

The `timbre_eval` integration test compares isolated rendered hits with two
download-on-demand reference sets. Audio stays under the ignored `target/`
directory and is not committed.

## Reference sets

`Virtuosity Drums` supplies eight hits from one recorded jazz kit. It is useful
as a real acoustic recording anchor, but represents one drum set, room and
microphone mix. Its library trims onsets and fades/caps tails; these files are
not raw microphone tracks.

`StemGMD` supplies the sixth of ten velocity layers for nine instruments from
ten different sample-library kits (90 stereo WAVs total). The test reports
median and range across kits so calibration does not mistake one drum model for
a universal target. These are rendered sample-library hits, not raw acoustic
recordings, but they provide useful variation in drum construction and
production sound.

## Download and run

```sh
python3 tools/download_drum_references.py
cargo nextest run -p physics-drum --test timbre_eval -- --nocapture
```

The downloader verifies the pinned archives using SHA-256 for Virtuosity and
the published MD5 for StemGMD. It decodes the Virtuosity FLAC files with
`ffmpeg`, extracts only the selected StemGMD hits, and writes per-file hashes
to manifests. The full StemGMD archive is about 200 MB; the extracted test
references are smaller. `--archive-dir` can reuse already-downloaded `.crate`
and `.zip` files. `--skip-virtuosity` prepares only StemGMD.

By default, the WAVs are stored in `target/drum-reference/virtuosity` and
`target/drum-reference/stemgmd`. Set `DRUM_REFERENCE_DIR` or
`DRUM_STEMGMD_REFERENCE_DIR` to use other locations.

The evaluation checks normalized spectral centroid, high-band energy and
decay descriptors. For the ten-kit set, the regression bounds allow up to a
2.2x centroid ratio, 0.18 high-band-ratio difference and 0.21 decay-ratio
difference from the median. It does not compare waveforms or absolute recording
levels. Instrument characteristics vary across kits; these are broad regression
targets for the physical model, not a claim that every kit should have the same
sound.

Sources:

- [Virtuosity Drums](https://github.com/sfzinstruments/virtuosity_drums)
- [Virtuosity core reference archive](https://docs.rs/crate/ferrosintesis-samples-drumkit/0.2.0)
- [Virtuosity cymbal reference archive](https://docs.rs/crate/ferrosintesis-samples-drumkit2/0.2.0)
- [StemGMD single hits, Zenodo record](https://zenodo.org/records/7882857)
- [StemGMD paper and project](https://zenodo.org/records/7882857)
