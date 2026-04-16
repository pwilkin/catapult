# Changelog

## [0.1.3] - 2026-04-16

### Fixed

- **macOS app unresponsive (issue #8)**: The debounce introduced in 0.1.2 did not fully resolve the issue. The root cause was the initial `isMaximized()` call on mount, which on macOS triggers a resize event, which calls `isMaximized()` again — an infinite loop. Removed the initial call entirely; the debounced resize handler already keeps the maximize indicator in sync.

- **TUI crash in logs tab (issue #13)**: After restarting the server, the new log file is shorter than the previous one. If the scroll position was beyond the end of the new log, the slice operation panicked with an out-of-range index. The scroll offset is now clamped to the new line count on every tick, with an additional guard in the render path.

## [0.1.2] - 2026-04-13

### Fixed

- **macOS app unresponsive (issue #8)**: Calling `isMaximized()` inside the window resize handler triggered an infinite resize event loop on macOS, freezing the entire UI. The check is now debounced so the loop cannot form.

- **`--parallel 1` not emitted (issue #11)**: The `--parallel` flag was only emitted when the value was greater than 1. Since llama.cpp defaults to 4 parallel slots when the flag is omitted, users could not explicitly request single-slot mode from the UI. The flag is now always emitted.

- **`--no-cont-batching` not emitted (issue #11)**: Disabling continuous batching in the UI had no effect — the `--no-cont-batching` flag was never passed to llama-server. It is now emitted when the toggle is off.

- **Virtual GPU selected over real GPU on Windows (issue #9)**: GPU detection via WMI returned all video adapters in arbitrary order, so virtual adapters (Hyper-V, Microsoft Basic Display, VMware, etc.) could be picked as the primary GPU. Virtual adapters are now filtered out when a real GPU is present.

- **Server process orphaned on GUI exit (issue #7)**: Closing the GUI window without stopping the server left llama-server running in the background with no way to reattach. A shutdown handler now terminates the server process when the app exits.

- **Zombie server processes in TUI (issue #7)**: Stopped llama-server processes lingered as zombies in the process table until the TUI itself exited. The child process handle is now properly dropped instead of leaked via `mem::forget`, and `waitpid` is called after the process is confirmed dead.

- **Console windows flashing on Windows (issue #10)**: Every child process spawned for hardware detection (PowerShell, nvidia-smi, etc.) opened a visible console window. All subprocess invocations now use `CREATE_NO_WINDOW` to suppress them.

## [0.1.1] - 2026-04-10

### Fixed

- **App icon**: Replaced placeholder purple square icons with a proper catapult icon across all platforms (PNG, ICO, ICNS) and added the missing web favicon SVG.

- **Per-backend runtime management**: Managed runtimes are now identified by both build number and backend (e.g., CUDA, Vulkan, ROCm). Previously, only the build number was used, which prevented users from installing and switching between multiple backends of the same build version. Downloading a new backend no longer removes existing backends for the same build. Auto-delete of old runtimes now only removes outdated versions of the same backend type.

- **mmproj download filename**: When downloading a vision projection (mmproj) file alongside a core model, the mmproj filename is now prefixed with the core model's base name (e.g., `Qwen2.5-VL-7B-mmproj-f16.gguf` instead of just `mmproj-f16.gguf`). This ensures the mmproj is correctly detected and paired with its companion model.

- **mmproj detection via GGUF metadata**: Vision projection files are now detected not only by filename (containing "mmproj") but also by GGUF metadata (`general.architecture == "clip"`). This fixes detection for mmproj files from repositories that don't include "mmproj" in the filename. Detected mmproj files are also excluded from the main installed models list.

- **Config erasure on runtime download**: The runtime download handler previously cloned the config before the async operation and wrote it back after completion, which could silently discard any concurrent config changes (e.g., model selection, preset saves) made while the download was in progress. The download now returns a structured result that is applied atomically to the live config under its mutex lock.

- **Config robustness**: If the config file fails to parse on startup, Catapult now backs it up to `config.json.bak` before falling back to defaults, preserving the original data for recovery. The `auto_check_updates` setting now correctly defaults to `true` for new installs (previously it could silently default to `false` if the field was absent from the JSON).

### Added

- **Custom runtime: source distribution auto-import**: When browsing for a custom runtime, Catapult now detects llama.cpp source distributions by the presence of `CMakeLists.txt`. All `llama-server` binaries found under the tree are automatically registered as individual custom runtime entries, making it easy to switch between build configurations (e.g., CUDA vs. Vulkan builds) from a local build tree.

- **One-click runtime update**: The "Update available" banner on the Runtime page now triggers the download inline and displays a progress bar in place, instead of redirecting to the releases browser. The releases browser remains available for manual version selection.

- **Scanning spinner**: A loading overlay is displayed while Catapult scans a selected directory for `llama-server` binaries, providing feedback for large source trees that take a moment to traverse.

## [0.1.0] - Initial release

First public release of Catapult, a GUI/TUI launcher for llama.cpp.
