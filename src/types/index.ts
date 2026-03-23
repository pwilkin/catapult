// ── Hardware ──────────────────────────────────────────────────────────────────

export interface SystemInfo {
  cpu_name: string;
  cpu_cores: number;
  cpu_threads: number;
  total_ram_mb: number;
  available_ram_mb: number;
  gpus: GpuInfo[];
  os: string;
  arch: string;
  available_backends: BackendInfo[];
  recommended_backend: string;
}

export interface GpuInfo {
  name: string;
  vram_mb: number;
  vendor: "nvidia" | "amd" | "intel" | "apple" | "unknown";
}

export interface BackendInfo {
  id: string;
  name: string;
  available: boolean;
  description: string;
}

export interface SuggestedConfig {
  n_gpu_layers: number;
  n_ctx: number;
  can_fit_fully_in_vram: boolean;
  total_usable_mb: number;
  notes: string[];
}

// ── Runtime ───────────────────────────────────────────────────────────────────

export interface RuntimeInfo {
  installed: boolean;
  build: number | null;
  backend: string | null;
  path: string | null;
  server_binary: string | null;
  runtime_type: "managed" | "custom" | "none";
}

export interface ManagedRuntimeInfo {
  build: number;
  tag_name: string;
  backend_id: string;
  backend_label: string;
  asset_name: string;
  dir_name: string;
  installed_at: number;
}

export interface CustomRuntimeInfo {
  label: string;
  binary_path: string;
}

export interface ReleaseInfo {
  tag_name: string;
  build: number;
  published_at: string;
  available_assets: AssetOption[];
}

export interface AssetOption {
  name: string;
  backend_id: string;
  backend_label: string;
  platform: string;
  download_url: string;
  size_mb: number;
  score: number;
}

export interface CustomBuild {
  binary_path: string;
  label: string;
}

// ── Models ────────────────────────────────────────────────────────────────────

export interface ModelInfo {
  id: string;
  name: string;
  repo_id: string;
  filename: string;
  path: string;
  size_bytes: number;
  quant: string | null;
  params_b: string | null;
  context_length: number | null;
  is_vision: boolean;
  mmproj_path: string | null;
  split_files: string[];
}

export interface RecommendedModel {
  repo_id: string;
  filename: string;
  name: string;
  description: string;
  params_b: number;
  family: string;
  quant: string;
  context: number | null;
  estimated_size_mb: number;
  installed: boolean;
  installed_path: string | null;
}

export interface HfModel {
  repo_id: string;
  name: string;
  author: string;
  tags: string[];
  files: HfFile[];
  downloads: number;
  likes: number;
}

export interface HfFile {
  filename: string;
  size_bytes: number;
  quant: string | null;
  download_url: string;
  is_split: boolean;
  split_parts: HfFilePart[];
}

export interface HfFilePart {
  filename: string;
  size_bytes: number;
  download_url: string;
}

export interface KnownOwner {
  id: string;
  description: string;
}

// ── Server ────────────────────────────────────────────────────────────────────

export interface ServerConfig {
  model_path: string;
  mmproj_path: string | null;
  host: string;
  port: number;
  n_ctx: number;
  n_gpu_layers: number;
  n_threads: number | null;
  flash_attn: string;
  cache_type_k: string;
  cache_type_v: string;
  temperature: number;
  top_k: number;
  min_p: number;
  top_p: number;
  n_predict: number;
  n_batch: number;
  n_ubatch: number;
  cont_batching: boolean;
  mlock: boolean;
  no_mmap: boolean;
  seed: number | null;
  rope_freq_scale: number | null;
  rope_freq_base: number | null;
  grp_attn_n: number | null;
  grp_attn_w: number | null;
  parallel: number;
  extra_params: Record<string, string>;
}

export type ServerStatus =
  | { type: "stopped" }
  | { type: "starting" }
  | { type: "running"; port: number; pid: number }
  | { type: "error"; message: string };

// ── Downloads ─────────────────────────────────────────────────────────────────

export interface DownloadProgress {
  id: string;
  bytes_downloaded: number;
  total_bytes: number;
  percent: number;
  status: string; // "downloading" | "extracting" | "done" | "error" | "paused" | "retrying (N/3)"
}

// ── Config ────────────────────────────────────────────────────────────────────

export interface AppConfig {
  managed_runtimes: ManagedRuntimeInfo[];
  custom_runtimes: CustomRuntimeInfo[];
  active_runtime: { type: "managed"; build: number } | { type: "custom"; index: number } | { type: "none" };
  auto_delete_old_runtimes: boolean;
  models_dir: string | null;
  model_dirs: string[];
  download_dir: string | null;
  last_update_check: number | null;
  latest_known_build: number | null;
  auto_check_updates: boolean;
  favorite_models: string[];
  selected_model: string | null;
  wizard_completed: boolean;
}
