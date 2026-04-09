# Catapult

A desktop launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp). Manages runtime versions, discovers and downloads models, configures the server with full parameter coverage, and provides an embedded chat interface — all without touching the command line.

Available in two interfaces:
- **GUI** — A Tauri v2 desktop application (Rust backend + React/TypeScript frontend)
<img width="1280" height="808" alt="catapult-ui" src="https://github.com/user-attachments/assets/a39fa2ae-d289-4bdd-a335-2d083666956c" />
- **TUI** — A terminal-based interface built with [ratatui](https://github.com/ratatui/ratatui) for those who prefer the command line
<img width="1212" height="684" alt="catapult-tui" src="https://github.com/user-attachments/assets/368ac91d-fe48-474c-8eca-ed46e5d79c6e" />

## Features

**Dual Interface**
- **GUI**: Full desktop experience with visual dashboards, tabbed configuration, and embedded WebUI
- **TUI**: Fast keyboard-driven terminal interface with the same feature set

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
- Vision model detection with automatic mmproj file pairing; vision models marked in the dashboard (eye icon in GUI, `V` marker in TUI)

**Server Configuration**
- Full llama.cpp server parameter coverage organized into tabbed UI (Context, Hardware, Sampling, Server, Chat, Advanced)
- Save and load named configuration presets; per-model preset memory (last-used preset auto-loads on model selection)
- Auto-import `presets.ini` from HuggingFace repos on model download (sampling parameters applied as a named preset)
- Process lifecycle management with log streaming
- One-click launch from the dashboard

**Chat**
- Embedded llama.cpp WebUI in-app via iframe (GUI) or via llama-cli (TUI)
- Pop-out to a separate window (GUI)

**First-Launch Wizard (GUI)**
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

# Run TUI (terminal interface)
npm run tui
# Or directly with cargo:
cargo run --manifest-path src-tauri/Cargo.toml --bin catapult-tui
```

## TUI Usage

The TUI provides the same functionality as the GUI in a terminal-friendly format.

### Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Navigate between fields/sections |
| `1-6` | Switch tabs (Dashboard, Runtime, Models, Server, Logs, Chat) |
| `d`, `r`, `m`, `s`, `l`, `c` | Quick-jump to tab by first letter |
| `↑/↓` | Navigate lists |
| `Enter` | Select/activate |
| `Space` | Toggle checkbox |
| `Esc` / `q` | Go back / quit |
| `?` | Show help |

### Tabs

- **Dashboard** — System overview, quick launch, runtime status
- **Runtime** — Download and switch between llama.cpp builds
- **Models** — Browse local models, search HuggingFace, manage downloads
- **Server** — Configure and start the llama-server with full parameter coverage
- **Logs** — Stream server output in real-time
- **Chat** — Launch llama-cli for terminal-based chat (TUI resumes after exit)

### Building the TUI

```bash
# Run directly
cargo run --manifest-path src-tauri/Cargo.toml --bin catapult-tui

# Build release binary
cargo build --manifest-path src-tauri/Cargo.toml --bin catapult-tui --release
# Binary will be at: src-tauri/target/release/catapult-tui
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
- **Frontend (GUI):** React, TypeScript, Vite, Tailwind CSS
- **Frontend (TUI):** [ratatui](https://github.com/ratatui/ratatui), crossterm, tui-input
- **Testing:** Vitest (frontend), `#[cfg(test)]` modules (backend)
- **CI:** GitHub Actions — tests on every push/PR, cross-platform builds on main/tags

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
