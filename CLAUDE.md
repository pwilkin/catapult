# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Catapult is a Tauri v2 desktop application (Rust backend + React/TypeScript frontend) that acts as a GUI launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp). It handles runtime version management, model discovery/download, server configuration with full parameter coverage, and provides an embedded chat interface. See `ARCHITECTURE.md` for detailed technical documentation.

## Commands

```bash
# Install frontend deps
npm install

# Dev mode (opens Tauri window with hot-reload)
npm run dev

# Production build
npm run build

# Frontend only (Vite dev server, no Tauri window)
npm run vite

# Type-check frontend
npx tsc --noEmit

# Check Rust
cargo check --manifest-path src-tauri/Cargo.toml

# Run Rust tests (55 tests)
cargo test --manifest-path src-tauri/Cargo.toml

# Run frontend tests (34 tests via Vitest)
npm test

# Watch mode for frontend tests
npm run test:watch
```

## Architecture

### IPC pattern
All file system, network, and process operations live in Rust. The frontend only calls `invoke()` commands and listens for events. No Tauri plugins needed for these; they go through the Rust backend. There are 47 registered Tauri commands.

### Rust backend (`src-tauri/src/`)
- `lib.rs` — registers all Tauri commands, owns `AppState`, CLI flag handling (`--force-wizard`)
- `config.rs` — `AppConfig` persisted at `{data_dir}/catapult/config.json`; runtime types (`ManagedRuntime`, `CustomRuntime`, `ActiveRuntime` enum)
- `hardware.rs` — cross-platform CPU/RAM/GPU detection, backend availability, config suggestions
- `runtime.rs` — GitHub releases API, asset scoring/selection, versioned download/extraction, managed/custom runtime operations
- `huggingface.rs` — HF API search/repo-files, curated `RECOMMENDED_MODELS` list, quant extraction
- `models.rs` — GGUF scanning (recursive, multi-directory), binary metadata parser (name/params/context/vision tags), metadata cache, download with resume/retry, mmproj auto-detection
- `server.rs` — `ServerConfig` with typed fields + `extra_params` HashMap, process spawn/kill with SIGTERM+timeout, stdout/stderr streaming, CLI arg builder

### Frontend (`src/`)
- React Router v6 with a persistent sidebar layout
- Pages: Wizard (first-launch) → Dashboard → Runtime → Models → Server → Chat
- `src/types/index.ts` mirrors all Rust `#[derive(Serialize, Deserialize)]` structs — **keep in sync manually**
- `src/utils/format.ts` — shared formatting utilities (sizes, CPU/GPU name shortening, quant colors)
- Chat page embeds the llama.cpp WebUI in an `<iframe>` pointing at `http://127.0.0.1:{port}`
- Events: `download_progress` (DownloadProgress), `server_log` (string)

### Data dirs (cross-platform via `dirs` crate)
- Config: `{data_dir}/catapult/config.json`
- GGUF cache: `{data_dir}/catapult/gguf_cache.json`
- Managed runtimes: `{data_dir}/catapult/runtimes/{build}-{backend}/`
- Legacy runtime: `{data_dir}/catapult/runtime/` (migrated on load)
- Models (default): `{data_dir}/catapult/models/`
- Presets: `{data_dir}/catapult/presets/`

### Runtime management
- **Managed runtimes**: Downloaded from GitHub in versioned subdirectories (`runtimes/b5000-cuda/`). Multiple versions coexist; one is active. Old versions optionally auto-deleted.
- **Custom runtimes**: User-pointed directories scanned recursively. Multiple can be registered.
- Active runtime switched via `set_active_runtime` command.

### Asset scoring (runtime.rs `score_asset`)
CUDA=100, Metal=95, ROCm=90, Vulkan=70, SYCL=60, CPU AVX-512=30, CPU AVX2=25. Unavailable backends penalized by -200.

### Server process management
`server.rs::start_server` spawns `llama-server` with `kill_on_drop(true)`. Child stored in `ServerState` (Mutex). Exit monitored via `try_wait()` polling. Stop sends SIGTERM, waits 30s, then SIGKILL. Full command line stored as first log entry.

### Server configuration
Core typed fields (model, host, port, context, GPU layers, sampling, etc.) plus `extra_params: HashMap<String, String>` for all additional llama-server flags. Frontend organizes into 6 tabs: Context, Hardware, Sampling, Server, Chat, Advanced. Presets saved as JSON files; `__default__` preset stores user defaults. Model path and mmproj excluded from presets.

### Model management
- Multiple GGUF directories scanned recursively with deduplication
- GGUF binary parser extracts name, params, context length, vision tags from file headers
- Metadata cached in `gguf_cache.json` (invalidated on size/mtime change)
- Vision models auto-paired with compatible mmproj files in same directory
- Downloads support HTTP Range resume, exponential backoff (5 retries), counter reset on progress
- Installed models: columnar table with sorting (name/params/quant/ctx/size), filtering, favorites

## Key constraints
- The Chat page iframes `http://127.0.0.1:{port}` — it loads llama.cpp's own built-in WebUI inside the Tauri window. No custom UI is served by Catapult.
- The CSP in `tauri.conf.json` must allow scripts, styles, connects, and WebSocket from `http://127.0.0.1:*` and `http://localhost:*` for the Chat iframe to work.
- `n_gpu_layers = -1` means "all layers on GPU". `0` means CPU-only.
- `--flash-attn` takes a parameter: `on`, `off`, or `auto` (default: `auto`).
- `--ctx-size 0` means "loaded from model" (the default).
- `seed: Option<u64>` in Rust — the UI shows `-1` for random, maps to `None` (omits `--seed` flag).
- Server presets exclude `model_path` and `mmproj_path` (per-session, not config).
- The `__raw__` key in `extra_params` holds free-form CLI arguments split by whitespace.
- GGUF metadata parsing is capped at 128 KV pairs and 1MB strings for safety.
- Model download temp files use `__downloading__` prefix and are preserved for resume.
