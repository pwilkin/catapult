# Architecture

Catapult is a Tauri v2 desktop application (Rust backend + React/TypeScript frontend) that serves as a GUI launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp). It handles runtime version management, model discovery, server configuration, and provides an embedded chat interface.

## Directory structure

```
catapult/
├── src-tauri/src/           # Rust backend (4,300+ LOC)
│   ├── lib.rs               # Tauri command registration, AppState, IPC handlers
│   ├── config.rs            # AppConfig persistence, runtime/model types
│   ├── hardware.rs          # CPU/RAM/GPU detection, backend scoring, config suggestions
│   ├── runtime.rs           # GitHub release fetching, asset scoring, download/extraction
│   ├── models.rs            # GGUF scanning, metadata parsing, model download with resume, split model consolidation
│   ├── server.rs            # ServerConfig, process spawn/kill, CLI arg builder, error retention
│   ├── huggingface.rs       # HF API search, recommended models list, quant extraction, recursive tree traversal
│   └── main.rs              # Entry point stub
├── src/                     # React/TypeScript frontend (4,400+ LOC)
│   ├── App.tsx              # Router (wizard + main layout)
│   ├── main.tsx             # React entry
│   ├── pages/
│   │   ├── Dashboard.tsx    # System overview, quick launch, favorite models
│   │   ├── Runtime.tsx      # Managed/custom runtime management, downloads
│   │   ├── Models.tsx       # Model browser, search, columnar list, directories
│   │   ├── Server.tsx       # Tabbed server configuration, presets, logs, session persistence
│   │   ├── Chat.tsx         # Embedded llama.cpp WebUI iframe
│   │   └── Wizard.tsx       # First-launch setup (runtime + model selection)
│   ├── components/
│   │   ├── Layout.tsx       # Sidebar navigation shell
│   │   └── CatapultIcon.tsx # SVG catapult icon
│   ├── types/index.ts       # TypeScript interfaces mirroring Rust structs
│   ├── utils/format.ts      # Shared formatting utilities (sizes, names, quants)
│   └── styles/globals.css   # Tailwind component classes
└── tests
    ├── (Rust)               # 37 unit tests in #[cfg(test)] modules
    └── src/utils/format.test.ts  # 34 Vitest unit tests
```

## IPC pattern

All filesystem, network, and process operations live in Rust. The frontend calls `invoke()` for request/response and `listen()` for streaming events. There are 42 registered Tauri commands spanning hardware detection, runtime management, model operations, server control, configuration, and presets.

**Events:**
- `download_progress` (DownloadProgress) — streamed during runtime and model downloads
- `server_log` (string) — each line of llama-server stdout/stderr

## Data directories

All paths are cross-platform via the `dirs` crate, relative to `dirs::data_dir()`:

```
{data_dir}/catapult/
├── config.json              # AppConfig (all settings)
├── gguf_cache.json          # GGUF metadata cache (path → name/params/ctx/vision)
├── runtimes/                # Managed runtime versions
│   ├── b5000-cuda/          # Versioned subdirectory per build+backend
│   └── b5100-cuda/
├── runtime/                 # Legacy single-runtime directory (migrated on load)
├── models/                  # Default model download directory
├── presets/                 # Server configuration presets (*.json)
│   └── __default__.json     # User-saved default settings
```

## Runtime management

Runtimes are either **managed** (downloaded from GitHub releases) or **custom** (user-pointed local installations).

### Managed runtimes
- Stored in versioned subdirectories: `runtimes/b{build}-{backend}/`
- Multiple versions can coexist; one is active at a time
- Old versions can be auto-deleted on new install (`auto_delete_old_runtimes` config flag)
- Non-active versions shown in a collapsible "archived" section
- Config tracks: build number, tag, backend ID/label, asset name, directory, install timestamp

### Custom runtimes
- Point to any directory containing a `llama-server` binary
- Scanning is recursive (depth 5) and detects multiple builds (e.g. `build/` + `vulkan/`)
- Multiple custom runtimes can be registered; one is active at a time

### Asset scoring
Each GitHub release asset is scored for the current platform: CUDA=100, Metal=95, ROCm=90, Vulkan=70, SYCL=60, CPU AVX-512=30, CPU AVX2=25, CPU AVX=20, CPU no-AVX=10. Backends not available on the system are penalized by -200.

## Model management

### Scanning
- Multiple GGUF storage directories can be configured, each scanned recursively (depth 5)
- A separate download directory is designated for new model downloads
- Imatrix/importance_matrix files are filtered from the display
- Split GGUF files (e.g. `model-00001-of-00003.gguf`) are consolidated into single logical model entries when all parts are present; incomplete sets show parts individually
- Deduplication by canonical path handles symlinks and overlapping directories
- `__downloading__` temp files are excluded from listings

### GGUF metadata
A binary parser reads GGUF v3 headers to extract:
- `general.name` — model name
- `general.size_label` — parameter count (e.g. "9.4B")
- `general.architecture` — used to locate context length key
- `{arch}.context_length` — training context window
- `general.tags` — string array; presence of "image-to-text" or "image-text-to-text" marks vision capability

Results are cached in `gguf_cache.json` keyed by file path, invalidated when file size or modification time changes. First scan reads headers; subsequent scans use cached data for near-instant loading.

### Vision models
Models tagged as vision-capable are paired with compatible mmproj files found in the same directory. Matching requires the mmproj filename to contain "mmproj" and share at least 2 name segments with the model (e.g. "Qwen3.5" + "4B"). The mmproj path is automatically passed as `--mmproj` when starting the server.

### Downloads
- HTTP Range resume support for interrupted downloads
- Exponential backoff retry: delays of 0s, 1s, 2s, 4s, 8s
- Consecutive failure counter resets when data is received (making flaky connections retry indefinitely as long as progress is made)
- After 5 consecutive failures: download pauses with Resume/Abort buttons
- Temp files (`__downloading__` prefix) preserved for resume across app restarts
- **Split/multipart models**: downloaded sequentially part-by-part with combined progress reporting; already-completed parts are skipped on resume; abort/delete cleans up all parts
- HuggingFace repo tree traversal is recursive (depth 3) to discover split models in subdirectories
- Active downloads are displayed in a persistent bar on the Models page regardless of active tab

## Server configuration

### ServerConfig
Core typed fields: model path, mmproj path, host, port, context size, GPU layers, threads, flash attention mode, KV cache types, sampling parameters (temperature, top-k/p, min-p, seed), batch sizes, memory flags (mlock, mmap), RoPE parameters, parallel slots.

All additional llama-server parameters are stored in `extra_params: HashMap<String, String>` where:
- Keys are CLI flag names without `--` prefix (e.g. "api-key", "timeout")
- Empty values represent boolean flags (emitted as just `--flag`)
- Non-empty values are emitted as `--flag value`
- Special key `__raw__` holds free-form CLI arguments split by whitespace
- The `mmproj` key is filtered from extra_params (handled as a typed field)

### Tabbed UI
Parameters are organized into 6 tabs: Context, Hardware, Sampling, Server, Chat, Advanced. The Advanced tab includes sub-sections for RoPE, speculative decoding, LoRA/control vectors, multimodal, CPU affinity, logging, and a raw arguments text field.

### Presets
Server configurations are saved as JSON files in `{data_dir}/catapult/presets/`. A special `__default__` preset stores user-customized defaults. Model path and mmproj path are excluded from presets (they're per-session). Loading a preset preserves the current model selection.

### Session persistence
Server configuration, active preset, and active tab are persisted to `sessionStorage` across page navigation within the same session. On initial load, state is restored from sessionStorage with fallback to saved defaults.

### Model selection
- The model list in the Run page is collapsible (shows selected model name when collapsed)
- Models are sorted with favorites first
- Selecting a model auto-applies suggested hardware settings (n_ctx, n_gpu_layers) without overriding user preferences

## Server process management

`start_server` spawns `llama-server` with `kill_on_drop(true)`. The child process is stored in `ServerState` (behind a Mutex). Stdout/stderr are read by independent tokio tasks (using manual `read_until` loops) that emit `server_log` events and buffer up to 500 lines. The full command line is stored as the first log entry.

Process exit is monitored by a polling task using `try_wait()` every 500ms. `stop_server` sends SIGTERM (Unix) or TerminateProcess (Windows), waits up to 30 seconds, then force-kills with SIGKILL if needed.

Status transitions: `Stopped → Starting → Running` (detected by "HTTP server listening" in output) or `Starting → Error` on process exit. On crash, error messages (exit code, process errors) are persisted to the log buffer and emitted as log events, ensuring error context is visible in the UI.

The frontend batches incoming log events via `requestAnimationFrame`, flushing accumulated lines once per frame to avoid performance issues with high-frequency output.

## First-launch wizard

A two-step onboarding flow at `/wizard` (outside the sidebar layout):
1. **System & Runtime** — hardware detection summary, runtime asset selection or custom directory browse, download with progress
2. **Model Selection** — recommended models filtered and sorted by hardware fit (VRAM/RAM), up to 3 selectable, parallel downloads

Controlled by `wizard_completed` in AppConfig. Skippable at any time. Re-runnable via `--force-wizard` CLI flag or programmatic reset.

## Chat

The Chat page embeds llama.cpp's built-in WebUI in an `<iframe>` pointing at `http://127.0.0.1:{port}`. A "Pop out" button opens it in a separate Tauri window. The CSP in `tauri.conf.json` allows scripts, styles, connections, and WebSocket from `http://127.0.0.1:*` and `http://localhost:*` to support the embedded SvelteKit app.

## Styling

- Tailwind CSS with a dark theme (custom colors via `tailwind.config.js`)
- Sharp borders throughout (no border-radius on rectangular elements)
- Circular elements (status dots, toggle switches, radio buttons) retain `rounded-full`
- Component classes: `.card`, `.btn-*`, `.input`, `.badge-*`, `.progress-bar`
- Quantization badges use a color gradient by precision: blue (F16/Q8/Q7) → cyan (Q6) → green (Q5) → yellow (Q4) → orange (Q3) → red (Q2) → dark red (Q1). MXFP quants are mapped to equivalent Q levels.
- Custom catapult SVG icon in the sidebar

## Testing

- **Rust (37 tests):** `cargo test` — unit tests in `#[cfg(test)]` modules covering asset scoring, backend detection, CLI arg building, quant extraction, size estimation, filename parsing, GGUF parsing, hardware config suggestions, split file parsing, imatrix detection, and split model consolidation
- **TypeScript (34 tests):** `npm test` (Vitest) — utility function tests for CPU/GPU name shortening, size formatting, quant color/sort mapping, imatrix detection, and MXFP quant handling
- Tests caught a real bug: `noavx` backend detection was unreachable due to `contains("avx")` matching first
