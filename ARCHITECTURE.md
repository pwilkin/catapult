# Architecture

Catapult is a dual-interface application serving as a launcher for [llama.cpp](https://github.com/ggml-org/llama.cpp). It handles runtime version management, model discovery, server configuration, and provides an embedded chat interface.

**Interfaces:**
- **GUI** — Tauri v2 desktop application (Rust backend + React/TypeScript frontend)
- **TUI** — Terminal-based interface (Rust + ratatui/crossterm)

## Directory structure

```
catapult/
├── src-tauri/src/           # Rust backend (~6,500 LOC including TUI)
│   ├── lib.rs               # Tauri command registration, AppState, IPC handlers
│   ├── config.rs            # AppConfig persistence, runtime/model types
│   ├── hardware.rs          # CPU/RAM/GPU detection, backend scoring, config suggestions
│   ├── runtime.rs           # GitHub release fetching, asset scoring, download/extraction
│   ├── models.rs            # GGUF scanning, metadata parsing, model download with resume
│   ├── server.rs            # ServerConfig, process spawn/kill, CLI arg builder
│   ├── huggingface.rs       # HF API search, recommended models, quant extraction, presets.ini fetch
│   ├── main.rs              # GUI entry point stub
│   ├── tui_main.rs          # TUI entry point (ratatui + crossterm)
│   └── tui/                 # TUI implementation (~2,200 LOC)
│       ├── app.rs           # TuiApp state, tab management, action handling
│       ├── event.rs         # Async event handler (key input + async events)
│       ├── server_ctl.rs    # TUI server lifecycle, PID file tracking
│       ├── params.rs        # Server parameter definitions for TUI forms
│       ├── tabs/            # Tab implementations
│       │   ├── dashboard.rs # System overview, quick actions
│       │   ├── runtime.rs   # Download/switch runtimes
│       │   ├── models.rs    # Model browser, HF search, downloads
│       │   ├── server.rs    # Server configuration forms
│       │   ├── logs.rs      # Real-time log streaming
│       │   └── chat.rs      # Launch llama-cli terminal chat
│       └── widgets/         # Reusable TUI components
│           ├── autocomplete.rs  # HuggingFace search autocomplete
│           └── progress.rs      # Progress bars for downloads
├── src/                     # React/TypeScript frontend (4,400+ LOC)
│   ├── App.tsx              # Router (wizard + main layout)
│   ├── main.tsx             # React entry
│   ├── pages/
│   │   ├── Dashboard.tsx    # System overview, quick launch, favorite models
│   │   ├── Runtime.tsx      # Managed/custom runtime management, downloads
│   │   ├── Models.tsx       # Model browser, search, columnar list, directories
│   │   ├── Server.tsx       # Tabbed server configuration, presets, logs
│   │   ├── Chat.tsx         # Embedded llama.cpp WebUI iframe
│   │   └── Wizard.tsx       # First-launch setup (runtime + model selection)
│   ├── components/
│   │   ├── Layout.tsx       # Sidebar navigation shell
│   │   └── CatapultIcon.tsx # SVG catapult icon
│   ├── types/index.ts       # TypeScript interfaces mirroring Rust structs
│   ├── utils/format.ts      # Shared formatting utilities
│   └── styles/globals.css   # Tailwind component classes
└── tests
    ├── (Rust)               # Unit tests in #[cfg(test)] modules
    └── src/utils/format.test.ts  # Vitest unit tests
```

## IPC pattern (GUI only)

All filesystem, network, and process operations live in Rust. The frontend calls `invoke()` for request/response and `listen()` for streaming events. There are 49 registered Tauri commands spanning hardware detection, runtime management, model operations, server control, configuration, presets, and per-model preset memory.

**Events:**
- `download_progress` (DownloadProgress) — streamed during runtime and model downloads
- `server_log` (string) — each line of llama-server stdout/stderr

The TUI does not use Tauri IPC; it directly calls the underlying library functions and manages its own async event loop.

## TUI Architecture

The TUI (`catapult-tui` binary) provides the same functionality as the GUI in a terminal-friendly format using [ratatui](https://github.com/ratatui/ratatui) for rendering and [crossterm](https://github.com/crossterm-rs/crossterm) for input handling.

### Entry Point
`tui_main.rs` sets up the terminal (raw mode, alternate screen), initializes the async event loop, and runs the main TUI loop. A panic hook ensures the terminal is restored on crash.

### Application State (`tui/app.rs`)
`TuiApp` holds all UI state across tabs:
- `config: AppConfig` — shared config with GUI
- `current_tab: Tab` — active tab (Dashboard, Runtime, Models, Server, Logs, Chat)
- Tab-specific state structs (e.g., `ModelsTabState`, `ServerTabState`) with inputs, lists, focus tracking
- `active_downloads: HashMap<String, ActiveDownload>` — ongoing runtime/model downloads
- `server_state: Option<DetectedServer>` — PID file detection of existing server processes
- `logs: Vec<String>` — captured server output

### Event System (`tui/event.rs`)
A tokio task handles both:
- **Synchronous input**: crossterm key events polled at 60Hz
- **Async events**: download progress, HTTP search results, server lifecycle events via an `mpsc` channel

Events are merged into a single `TuiEvent` enum for the main loop.

### Tabs (`tui/tabs/`)
Each tab is a module with:
- `render()` function for drawing the tab content
- `handle_input()` function for key event handling
- Optional `on_*` callbacks for async results (e.g., `on_search_results`)

Tabs mirror the GUI pages: Dashboard, Runtime, Models, Server, Logs, Chat.

### Server Control (`tui/server_ctl.rs`)
The TUI manages the server lifecycle independently of the GUI:
- PID file at `{data_dir}/catapult/.server.pid` tracks running servers
- `detect_existing_server()` checks for orphaned processes on startup
- Server started/stopped via direct process spawn (not Tauri commands)
- Logs captured via tokio `tokio::io::AsyncBufReadExt` lines

### Chat Mode
Unlike the GUI which embeds a web UI, the TUI's Chat tab launches `llama-cli` as an interactive subprocess, suspending the TUI until the user exits (Ctrl-C or `/exit`). The terminal is restored to cooked mode during the chat session.

### Shared Core
The TUI reuses all core Rust modules:
- `config::AppConfig` — same config file, same persistence
- `runtime` — same download/extraction logic
- `models` — same GGUF scanning and metadata parsing
- `huggingface` — same HF API integration
- `server` — same `ServerConfig` and CLI arg building

Only the presentation layer (ratatui vs React) differs.

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

The Advanced tab (GUI) and TUI params cover an extended set of parameters including: MoE CPU offloading (`cpu-moe`, `n-cpu-moe`), weight repacking (`no-repack`), host tensor offload (`no-op-offload`), device bypass (`no-host`), memory auto-fitting (`--fit`, `--fit-margin`, `--fit-ctx`), KV unified buffer (`kv-unified`), N-gram speculation (`spec-ngram-size-n/m`, `spec-ngram-min-hits`), lookup cache files, draft model threading/device params, built-in tools (`tools`), embedding/classification separators, WebUI config overrides, and `reuse-port`.

All additional llama-server parameters are stored in `extra_params: HashMap<String, String>` where:
- Keys are CLI flag names without `--` prefix (e.g. "api-key", "timeout")
- Empty values represent boolean flags (emitted as just `--flag`)
- Non-empty values are emitted as `--flag value`
- Special key `__raw__` holds free-form CLI arguments split by whitespace
- The `mmproj` key is filtered from extra_params (handled as a typed field)

### Tabbed UI (GUI)
Parameters are organized into 6 tabs: Context, Hardware, Sampling, Server, Chat, Advanced. The Advanced tab includes sub-sections for RoPE, speculative decoding, LoRA/control vectors, multimodal, CPU affinity, logging, and a raw arguments text field.

### TUI Forms
Parameters are edited via inline text inputs within tab panels. Navigation uses Tab/Shift+Tab to move between fields. Checkboxes are toggled with Space. The Server tab provides fields for the most common parameters; advanced parameters can be added via the `extra_params` HashMap editor.

The TUI Server tab tracks the `current_preset` name (`Option<String>`). On model selection, `load_preset_for_model()` looks up the model's saved preset from `AppConfig.model_presets`, falling back to `__default__`. When a server is started, the active preset is persisted to `model_presets`.

### Presets
Server configurations are saved as JSON files in `{data_dir}/catapult/presets/`. A special `__default__` preset stores user-customized defaults. Model path and mmproj path are excluded from presets (they're per-session). Loading a preset preserves the current model selection.

**Per-model preset memory**: Each model can have a last-used preset associated with it. This association is stored in `AppConfig.model_presets` (`HashMap<String, String>`, keyed by model file path). When a model is selected, its saved preset is auto-loaded. When a preset is applied and a server is started, the model→preset association is persisted. Two new Tauri commands support this: `get_model_preset` and `set_model_preset`.

**HuggingFace `presets.ini` auto-import**: On successful model download, Catapult fetches `presets.ini` from the HF repo (if it exists) and saves it as a named preset (repo ID with `/` replaced by `__`). The file is parsed for sampling parameters: temperature, top-k/p, min-p, n-predict, seed, repeat-penalty, repeat-last-n. This is handled by `huggingface::fetch_presets_ini()` and `server::apply_hf_preset_params()`.

### Session persistence (GUI only)
Server configuration, active preset, and active tab are persisted to `sessionStorage` across page navigation within the same session. On initial load, state is restored from sessionStorage with fallback to saved defaults.

The TUI does not persist session state; each launch starts fresh with the Dashboard tab active.

### Model selection (GUI)
- The model list in the Run page is collapsible (shows selected model name when collapsed)
- Models are sorted with favorites first; vision models display an eye icon
- Selecting a model checks for a saved preset (`get_model_preset`); if found, the preset is loaded instead of hardware suggestions. Otherwise, auto-applies suggested hardware settings (n_ctx, n_gpu_layers) without overriding user preferences

### Model selection (TUI)
- Models are selected via arrow navigation in a scrollable list
- Favorites are shown first with a `★` marker; vision models are flagged with a `V` marker (cyan)
- Selecting a model updates the server config immediately and auto-loads the model's last-used preset via `load_preset_for_model()`
- No auto-collapse; the list remains visible for re-selection

## Server process management

`start_server` spawns `llama-server` with `kill_on_drop(true)`. The child process is stored in `ServerState` (behind a Mutex). Stdout/stderr are read by independent tokio tasks (using manual `read_until` loops) that emit `server_log` events and buffer up to 500 lines. The full command line is stored as the first log entry.

Process exit is monitored by a polling task using `try_wait()` every 500ms. `stop_server` sends SIGTERM (Unix) or TerminateProcess (Windows), waits up to 30 seconds, then force-kills with SIGKILL if needed.

Status transitions: `Stopped → Starting → Running` (detected by "HTTP server listening" in output) or `Starting → Error` on process exit. On crash, error messages (exit code, process errors) are persisted to the log buffer and emitted as log events, ensuring error context is visible in the UI.

The GUI frontend batches incoming log events via `requestAnimationFrame`, flushing accumulated lines once per frame to avoid performance issues with high-frequency output. The TUI renders logs directly in its draw loop without batching.

## First-launch wizard (GUI only)

A two-step onboarding flow at `/wizard` (outside the sidebar layout):
1. **System & Runtime** — hardware detection summary, runtime asset selection or custom directory browse, download with progress
2. **Model Selection** — recommended models filtered and sorted by hardware fit (VRAM/RAM), up to 3 selectable, parallel downloads

Controlled by `wizard_completed` in AppConfig. Skippable at any time. Re-runnable via `--force-wizard` CLI flag or programmatic reset.

The TUI does not have a wizard; all functionality is accessible through the tabbed interface immediately on launch.

## Chat

### GUI Chat
The Chat page embeds llama.cpp's built-in WebUI in an `<iframe>` pointing at `http://127.0.0.1:{port}`. A "Pop out" button opens it in a separate Tauri window. The CSP in `tauri.conf.json` allows scripts, styles, connections, and WebSocket from `http://127.0.0.1:*` and `http://localhost:*` to support the embedded SvelteKit app.

### TUI Chat
The TUI Chat tab launches `llama-cli` as an interactive subprocess with the currently selected model. The TUI suspends its interface (restoring normal terminal mode), launches the CLI chat, and resumes when the user exits (Ctrl-C or `/exit`). This provides a terminal-native chat experience without a separate WebUI.

## Styling

### GUI Styling
- Tailwind CSS with a dark theme (custom colors via `tailwind.config.js`)
- Sharp borders throughout (no border-radius on rectangular elements)
- Circular elements (status dots, toggle switches, radio buttons) retain `rounded-full`
- Component classes: `.card`, `.btn-*`, `.input`, `.badge-*`, `.progress-bar`
- Quantization badges use a color gradient by precision: blue (F16/Q8/Q7) → cyan (Q6) → green (Q5) → yellow (Q4) → orange (Q3) → red (Q2) → dark red (Q1). MXFP quants are mapped to equivalent Q levels.
- Custom catapult SVG icon in the sidebar

### TUI Styling
- Ratatui's default styling with custom color schemes
- Focus indicators via border highlighting
- Progress bars for downloads using Unicode block characters
- Modal dialogs for confirmations and help text

## Testing

- **Rust:** `cargo test` — 55 unit tests in `#[cfg(test)]` modules covering asset scoring, backend detection, CLI arg building, quant extraction, size estimation, filename parsing, GGUF parsing, hardware config suggestions, split file parsing, imatrix detection, split model consolidation, `presets.ini` parsing, `apply_hf_preset_params`, preset name derivation, and `AppConfig.model_presets` round-tripping. TUI modules share the same core logic and are tested through the underlying library functions.
- **TypeScript:** `npm test` (Vitest) — 34 utility function tests for CPU/GPU name shortening, size formatting, quant color/sort mapping, imatrix detection, and MXFP quant handling
- Tests caught a real bug: `noavx` backend detection was unreachable due to `contains("avx")` matching first
