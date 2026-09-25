//! Standalone Desktop Application with Hardware-Accelerated GUI and Audio/MIDI for Physics Piano.

use nih_plug::prelude::*;
use physics_piano::nih_plugin::PhysicsPiano;

fn main() {
    nih_export_standalone::<PhysicsPiano>();
}
