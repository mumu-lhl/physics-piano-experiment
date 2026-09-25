"""Objective acoustic physical verification suite.

Implements all metrics specified in the industrial piano physical modeling verification pipeline:
  1. Inharmonicity B factor regression & cent deviation sigma(Delta C)
  2. Schroeder EDC two-stage decay ratio (tau_after / tau_prompt)
  3. 1/1 Octave band filter bank decay RMSE_T60
  4. Non-linear hammer contact duration ladder & monotonic contraction
  5. Dynamic spectral centroid scaling slope kappa_dyn in [0.45, 0.65]
  6. Attack transient envelope rise time Delta t_10-90
  7. Multi-Resolution STFT Loss (MRSL)
"""

from physics_piano.metrics.inharmonicity import estimate_inharmonicity_from_audio
from physics_piano.metrics.decay_edc import compute_schroeder_edc, analyze_two_stage_decay
from physics_piano.metrics.contact_time import measure_hammer_contact_time
from physics_piano.metrics.dynamic_centroid import measure_dynamic_centroid_slope, compute_spectral_centroid
from physics_piano.metrics.transient import analyze_transient_onset
from physics_piano.metrics.mrsl import compute_mrsl
from physics_piano.metrics.octave_decay import analyze_octave_t60

__all__ = [
    "estimate_inharmonicity_from_audio",
    "compute_schroeder_edc",
    "analyze_two_stage_decay",
    "measure_hammer_contact_time",
    "measure_dynamic_centroid_slope",
    "compute_spectral_centroid",
    "analyze_transient_onset",
    "compute_mrsl",
    "analyze_octave_t60",
]
