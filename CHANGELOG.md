# Changelog

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
