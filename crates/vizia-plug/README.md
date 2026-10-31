# Vizia Plug

A replacement for the old `nih-plug-vizia` which updates it to the latest version of `vizia`.

This crate allows for the use of the Vizia GUI library to be used with the [nice-plug](https://codeberg.org/BillyDM/nice-plug.git) (formerly NIH-plug) plugin framework.

## Building Plugins

This workspace uses the stable Rust toolchain (Rust 2024 Edition) and the
`xtask` bundler. To bundle an instrument's CLAP and VST3 plugins:

```
cargo xtask bundle physics-piano --release
```

The outputs will be placed in the `target/bundled` directory. The
instrument name can be `physics-piano`, `physics-guitar`, or
`physics-bass`. There is no `gain_gui` example in this workspace.
