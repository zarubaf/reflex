# Building from Source

## Prerequisites

- **Rust** stable toolchain ([install](https://rustup.rs/))
- **Git** with submodule support

### macOS

No additional dependencies needed. GPUI uses native Metal rendering.

### Linux

Install the following system libraries:

```bash
sudo apt-get install -y \
    libwayland-dev \
    libxkbcommon-x11-dev \
    libvulkan-dev \
    libfontconfig1-dev \
    libfreetype6-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxcb-cursor-dev \
    libxcb-render0-dev \
    libxcb-randr0-dev \
    libxcb-keysyms1-dev \
    libxcb-xkb-dev \
    libxkbcommon-dev \
    libglib2.0-dev \
    libatk1.0-dev \
    libgtk-3-dev \
    libclang-dev \
    cmake
```

## Clone

```bash
git clone --recurse-submodules https://github.com/zarubaf/reflex.git
cd reflex
```

If you already cloned without `--recurse-submodules`:

```bash
git submodule update --init --recursive
```

## Build

```bash
cargo build              # Debug build
cargo build --release     # Release build (recommended for large traces)
```

## Run

```bash
cargo run --release -- path/to/trace.uscope
```

## Format and Lint

```bash
cargo fmt                 # Format code
cargo clippy              # Lint
cargo test                # Run tests
```

## CI

The project uses GitHub Actions for CI:

- **Build** — macOS and Linux release builds
- **Clippy** — Lint with `-D warnings`
- **Format** — `cargo fmt --check`
- **Test** — `cargo test`

All CI jobs use `actions/checkout@v4` with `submodules: recursive` to handle the µScope submodule.

## Release

Tagged releases (`v*`) trigger the release workflow:
- macOS: `.app` bundle, signed and notarized
- Linux: standalone binary tarball
- Both uploaded as GitHub release artifacts
