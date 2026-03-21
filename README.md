# Catapult

A desktop GUI launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp). Manages runtime versions, discovers and downloads models, configures the server with full parameter coverage, and provides an embedded chat interface — all without touching the command line.

Built with [Tauri v2](https://v2.tauri.app/) (Rust backend + React/TypeScript frontend).

## Features

**Runtime Management**
- Download managed llama.cpp builds from GitHub releases with automatic platform/backend detection
- Multiple versions can coexist; switch between them instantly
- Point to existing local llama.cpp installations (custom runtimes)
- Backend scoring: automatically recommends CUDA, Metal, ROCm, Vulkan, or CPU builds based on your hardware

**Model Management**
- Scan multiple local directories for GGUF models with recursive discovery
- Parse GGUF metadata (name, parameter count, context length, vision capability) directly from file headers
- Download models from HuggingFace with resume support and exponential backoff retry
- Curated list of recommended models filtered by your hardware
- Favorites, sorting, filtering, and quant-level color coding
- Vision model detection with automatic mmproj file pairing

**Server Configuration**
- Full llama.cpp server parameter coverage organized into tabbed UI (Context, Hardware, Sampling, Server, Chat, Advanced)
- Save and load named configuration presets
- Process lifecycle management with log streaming
- One-click launch from the dashboard

**Chat**
- Embedded llama.cpp WebUI in-app via iframe
- Pop-out to a separate window

**First-Launch Wizard**
- Hardware detection and runtime recommendation
- Model selection with hardware fit indicators
- Get from zero to chatting in under a minute

## Download

Pre-built binaries for Linux, macOS (Universal), and Windows are available on the [Releases](../../releases) page.

| Platform | Format |
|----------|--------|
| Linux    | AppImage, .deb |
| macOS    | .dmg (Universal: Intel + Apple Silicon) |
| Windows  | .msi |

## Building from Source

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- Platform-specific dependencies (see below)

#### Linux

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev patchelf
```

#### macOS / Windows

No additional system dependencies required.

### Build

```bash
# Install frontend dependencies
npm install

# Development mode (opens Tauri window with hot-reload)
npm run dev

# Production build (outputs to src-tauri/target/release/bundle/)
npm run build
```

## Testing

```bash
# Frontend tests (Vitest)
npm test

# Rust tests
cargo test --manifest-path src-tauri/Cargo.toml

# Type-check frontend
npx tsc --noEmit
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed technical documentation covering the IPC pattern, data directories, runtime/model/server subsystems, and more.

## Tech Stack

- **Backend:** Rust, Tauri v2, Tokio, Reqwest, Serde
- **Frontend:** React, TypeScript, Vite, Tailwind CSS
- **Testing:** Vitest (frontend), `#[cfg(test)]` modules (backend)
- **CI:** GitHub Actions — tests on every push/PR, cross-platform builds on main/tags

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
