"""Analysis of hammer contact duration and dynamic spectral centroid scaling."""

import math
import numpy as np
from typing import Dict, Any, List

from physics_piano.params.schema import StringPhysicalParameters, HammerPhysicalParameters
from physics_piano.core.string import StiffStringModal
from physics_piano.core.hammer import HuntCrossleyHammer


def measure_hammer_contact_time(
    string_param: StringPhysicalParameters,
    hammer_param: HammerPhysicalParameters,
    velocities: List[float],
    sample_rate: float = 48000.0
) -> Dict[str, Any]:
    """Measure contact duration across velocity ladder and test monotonic contraction.
    
    Real physics criterion: Higher strike velocity compresses the nonlinear felt deeper,
    shortening contact time tau_contact(v0).
    """
    durations_ms = []

    for v in velocities:
        st = StiffStringModal(string_param, sample_rate)
        hm = HuntCrossleyHammer(hammer_param, sample_rate)
        hm.strike(v)

        contact_samples = 0
        for _ in range(int(sample_rate * 0.05)):  # 50 ms max simulation window
            u_s, v_s = st.get_strike_displacement_and_velocity()
            f = hm.compute_force(u_s, v_s)
            if f > 0.0:
                contact_samples += 1
            st.step(f)
            hm.advance(f)
            if hm.has_struck and not hm.is_active:
                break

        dur_ms = (contact_samples / sample_rate) * 1000.0
        durations_ms.append(float(dur_ms))

    # Verify monotonic decreasing trend: tau(v_{i+1}) <= tau(v_i)
    is_monotonic = all(durations_ms[i] >= durations_ms[i + 1] for i in range(len(durations_ms) - 1))
    contraction_ratio = durations_ms[0] / max(1e-4, durations_ms[-1])

    return {
        "velocities": velocities,
        "contact_times_ms": durations_ms,
        "is_monotonic_contracting": is_monotonic,
        "contraction_ratio": float(contraction_ratio),
        "passed": (is_monotonic and contraction_ratio >= 1.2)
    }
