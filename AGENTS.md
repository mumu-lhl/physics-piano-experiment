# Agent Guidelines & Workflow Instructions

Welcome to the **Physics Piano, Guitar, Bass & Drum** repository. When working as an automated assistant or developer on this codebase, adhere to the guidelines below.

---

## 🚀 Testing Guidelines: Use `cargo nextest`

> [!IMPORTANT]
> **Always use `cargo nextest` instead of standard `cargo test` when executing tests.**

This workspace contains extensive acoustic simulations, FDTD grid calculations, and physical modeling tests across multiple instrument crates. `cargo-nextest` provides per-test process isolation, multithreaded parallel execution, cleaner output, and dramatically faster completion times.

### Quick Reference

| Task | Recommended Command |
| :--- | :--- |
| **Run all workspace tests** | `cargo nextest run --workspace` |
| **Run tests for a single crate** | `cargo nextest run -p <crate_name>` (e.g. `cargo nextest run -p physics-piano`) |
| **Run a specific test by name** | `cargo nextest run <test_filter>` |
| **Run tests without fail-fast** | `cargo nextest run --workspace --no-fail-fast` |
| **Run documentation tests** | `cargo test --doc` *(Nextest does not execute doctests; use standard cargo only for `--doc`)* |

### Non-Interactive / Automation Tip

When running tests in automated environments or headless background tasks:
```bash
CI=1 cargo nextest run --workspace
```
Setting `CI=1` prevents nextest from entering interactive terminal/pager modes.

---

## 🛠️ Build and Verification Workflow

Before submitting or committing changes, run the following verification steps:

1. **Compilation Check across All Targets**:
   ```bash
   cargo check --workspace --all-targets
   ```
2. **Execute Full Test Suite**:
   ```bash
   CI=1 cargo nextest run --workspace
   ```
3. **Run Doctests (if library documentation changed)**:
   ```bash
   cargo test --doc
   ```

---

## 📁 Repository Overview

- `crates/physics-piano`: 88-key grand piano physical modeling engine (SAV hammer dynamics, multi-string coupling, UPOLS convolution, Vizia GUI).
- `crates/physics-guitar`: 6-string acoustic & electric guitar synthesis (tension modulation, tube amp emulation, cabinet IRs, real-time fretboard visualizer).
- `crates/physics-bass`: FDTD bass physical synthesis (CFL-bounded stiff string, fret crown curvature, slap/pop/arco mechanics).
- `crates/physics-drum`: Hybrid modal drum kit (Bessel membrane modes, coupled double-head, snare wire spatial contact).
- `crates/physics-presets`: Shared preset serialization, undo/redo manager, and factory preset databases.
- `crates/physics-ui`: Shared Vizia UI components, localization (en-US / zh-CN), and parameter widgets.
- `crates/vizia-plug`: In-tree adapter bridging `vizia` to `nice-plug` (`nice-plug-core 0.4.2`).
- `xtask`: Plugin bundler and build automation (`cargo xtask bundle`).
