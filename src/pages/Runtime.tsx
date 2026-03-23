import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Download,
  RefreshCw,
  CheckCircle,
  FolderOpen,
  ChevronDown,
  ChevronUp,
  Zap,
  Package,
  Trash2,
  Play,
  Archive,
} from "lucide-react";
import type {
  RuntimeInfo,
  ReleaseInfo,
  AssetOption,
  BackendInfo,
  CustomBuild,
  DownloadProgress,
  AppConfig,
} from "../types";

function mbToStr(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}

export default function Runtime() {
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const [appConfig, setAppConfig] = useState<AppConfig | null>(null);
  const [release, setRelease] = useState<ReleaseInfo | null>(null);
  const [backends, setBackends] = useState<BackendInfo[]>([]);
  const [selectedAsset, setSelectedAsset] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showReleases, setShowReleases] = useState(false);
  const [showAll, setShowAll] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const [customBuilds, setCustomBuilds] = useState<CustomBuild[] | null>(null);

  const loadData = async () => {
    try {
      const [rt, bk, cfg] = await Promise.all([
        invoke<RuntimeInfo>("get_runtime_info"),
        invoke<BackendInfo[]>("get_available_backends"),
        invoke<AppConfig>("get_config"),
      ]);
      setRuntime(rt);
      setBackends(bk);
      setAppConfig(cfg);
    } catch (e) {
      setError(String(e));
    }
  };

  const checkUpdate = async () => {
    setChecking(true);
    setError(null);
    try {
      const rel = await invoke<ReleaseInfo>("check_latest_release");
      setRelease(rel);
      if (rel.available_assets.length > 0) {
        setSelectedAsset(rel.available_assets[0].name);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setChecking(false);
    }
  };

  const startDownload = async () => {
    if (!selectedAsset) return;
    setDownloading(true);
    setError(null);
    setProgress({ id: "runtime", bytes_downloaded: 0, total_bytes: 0, percent: 0, status: "downloading" });
    const unlisten = await listen<DownloadProgress>("download_progress", (e) => {
      setProgress(e.payload);
    });
    try {
      await invoke("download_runtime", { assetName: selectedAsset });
      await loadData();
      setProgress(null);
      setShowReleases(false);
    } catch (e) {
      setError(String(e));
    } finally {
      unlisten();
      setDownloading(false);
    }
  };

  const cancelDownload = () => {
    setDownloading(false);
    setProgress(null);
  };

  const browseCustom = async () => {
    const selected = await open({ directory: true, title: "Select llama.cpp directory" });
    if (!selected) return;
    try {
      const builds = await invoke<CustomBuild[]>("scan_custom_runtime", { path: selected });
      if (builds.length === 0) {
        setCustomBuilds(null);
      } else if (builds.length === 1) {
        await invoke("set_custom_runtime_binary", { binaryPath: builds[0].binary_path });
        setCustomBuilds(null);
        await loadData();
      } else {
        setCustomBuilds(builds);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const selectBuild = async (build: CustomBuild) => {
    try {
      await invoke("set_custom_runtime_binary", { binaryPath: build.binary_path });
      await loadData();
    } catch (e) {
      setError(String(e));
    }
  };

  const activateManaged = async (build: number) => {
    try {
      await invoke("set_active_runtime", { runtimeType: "managed", id: build });
      await loadData();
    } catch (e) {
      setError(String(e));
    }
  };

  const activateCustom = async (index: number) => {
    try {
      await invoke("set_active_runtime", { runtimeType: "custom", id: index });
      await loadData();
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteManaged = async (build: number) => {
    try {
      await invoke("delete_managed_runtime", { build });
      await loadData();
    } catch (e) {
      setError(String(e));
    }
  };

  const removeCustom = async (index: number) => {
    try {
      await invoke("remove_custom_runtime", { index });
      await loadData();
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleAutoDelete = async (enabled: boolean) => {
    try {
      await invoke("set_auto_delete_runtimes", { enabled });
      await loadData();
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    loadData().then(() => checkUpdate());
  }, []);

  const managed = appConfig?.managed_runtimes ?? [];
  const custom = appConfig?.custom_runtimes ?? [];
  const activeType = appConfig?.active_runtime?.type ?? "none";
  const activeBuild = activeType === "managed" ? (appConfig?.active_runtime as { build: number }).build : null;
  const activeCustomIdx = activeType === "custom" ? (appConfig?.active_runtime as { index: number }).index : null;

  const latestBuild = release?.build;
  const updateAvailable = activeBuild != null && latestBuild != null && latestBuild > activeBuild;

  // Split managed runtimes: archived = older than latest release
  const archivedMr = latestBuild != null
    ? managed.filter((r) => r.build < latestBuild)
    : [];
  const currentMr = managed.filter((r) => !archivedMr.includes(r));

  const displayedAssets = showAll
    ? release?.available_assets ?? []
    : (release?.available_assets ?? []).slice(0, 5);

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-gray-100">Runtime</h1>
        <p className="text-gray-500 text-sm mt-1">
          Manage llama.cpp inference engine versions
        </p>
      </div>

      {error && (
        <div className="card border-accent-red/30 bg-accent-red/5">
          <p className="text-sm text-accent-red">{error}</p>
        </div>
      )}

      {/* ── Active Runtime ── */}
      <div className="card">
        <h2 className="section-title">Active Runtime</h2>
        {runtime?.installed ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <CheckCircle size={14} className="text-accent-green" />
              <span className="text-sm text-gray-200">
                {runtime.runtime_type === "managed" ? (
                  <>Build <span className="font-mono">b{runtime.build}</span></>
                ) : (
                  "Custom"
                )}
                {runtime.backend && (
                  <span className="uppercase text-xs text-primary-light font-medium ml-2">
                    {runtime.backend}
                  </span>
                )}
              </span>
              <span className={`badge-${runtime.runtime_type === "managed" ? "purple" : "gray"} text-[10px]`}>
                {runtime.runtime_type}
              </span>
            </div>
            {runtime.path && (
              <p className="text-xs text-gray-500 font-mono">{runtime.path}</p>
            )}
            {updateAvailable && (
              <div className="flex items-center gap-2 mt-2 px-3 py-2 border border-accent-yellow/30 bg-accent-yellow/5">
                <Package size={13} className="text-accent-yellow" />
                <span className="text-xs text-accent-yellow">
                  Update available: <span className="font-mono">b{latestBuild}</span>
                </span>
                <button className="btn-primary text-xs ml-auto" onClick={() => setShowReleases(true)}>
                  <Download size={12} /> Update
                </button>
              </div>
            )}
          </div>
        ) : (
          <div className="flex items-center justify-between">
            <p className="text-sm text-gray-500">No runtime configured</p>
            <div className="flex gap-2">
              <button className="btn-primary text-xs" onClick={() => setShowReleases(true)}>
                <Download size={12} /> Download
              </button>
              <button className="btn-ghost text-xs" onClick={browseCustom}>
                <FolderOpen size={12} /> Custom
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── Custom build picker ── */}
      {customBuilds && customBuilds.length > 1 && (
        <div className="card">
          <div className="flex items-start justify-between">
            <div>
              <h2 className="section-title">Multiple builds found</h2>
              <p className="section-desc">Select which llama-server build to use:</p>
            </div>
            <button className="btn-ghost text-xs" onClick={() => setCustomBuilds(null)}>Dismiss</button>
          </div>
          <div className="space-y-1.5">
            {customBuilds.map((b) => (
              <button key={b.binary_path}
                className="w-full flex items-center gap-3 px-3 py-2.5 border border-border hover:border-border-strong hover:bg-surface-3 text-left transition-colors"
                onClick={() => selectBuild(b)}>
                <Zap size={14} className="text-primary-light shrink-0" />
                <span className="text-sm font-mono text-gray-200 truncate">{b.label}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* ── Managed Runtimes ── */}
      <div className="card">
        <div className="flex items-center justify-between mb-3">
          <h2 className="section-title mb-0">Managed Runtimes</h2>
          <div className="flex items-center gap-2">
            {managed.length > 0 && (
              <label className="flex items-center gap-1.5 text-xs text-gray-500 cursor-pointer">
                <input type="checkbox" checked={appConfig?.auto_delete_old_runtimes ?? false}
                  onChange={(e) => toggleAutoDelete(e.target.checked)}
                  className="accent-primary" />
                Auto-delete old
              </label>
            )}
            <button className="btn-secondary text-xs" onClick={() => setShowReleases(true)}>
              <Download size={12} /> {managed.length > 0 ? "New version" : "Download"}
            </button>
          </div>
        </div>

        {managed.length === 0 ? (
          <p className="text-sm text-gray-500">No managed runtimes installed. Download one from GitHub releases.</p>
        ) : (
          <>
            {/* Current managed runtimes */}
            <div className="space-y-1.5">
              {currentMr.map((r) => {
                const isActive = r.build === activeBuild;
                return (
                  <div key={r.build}
                    className={`flex items-center gap-3 px-3 py-2.5 border ${
                      isActive ? "border-primary/40 bg-primary/5" : "border-border"
                    }`}>
                    {isActive && <CheckCircle size={14} className="text-primary shrink-0" />}
                    <span className={`text-sm font-mono ${isActive ? "text-gray-200" : "text-gray-400"}`}>b{r.build}</span>
                    <span className={`text-xs uppercase ${isActive ? "text-primary-light" : "text-gray-500"}`}>{r.backend_label}</span>
                    {isActive ? (
                      <span className="badge-purple text-[10px] ml-auto">Active</span>
                    ) : (
                      <>
                        <span className="text-xs text-gray-600 ml-auto">
                          {new Date(r.installed_at * 1000).toLocaleDateString()}
                        </span>
                        <button className="btn-ghost text-xs py-0.5 px-1.5"
                          onClick={() => activateManaged(r.build)} title="Activate">
                          <Play size={11} />
                        </button>
                        <button className="text-gray-600 hover:text-accent-red"
                          onClick={() => deleteManaged(r.build)} title="Delete">
                          <Trash2 size={12} />
                        </button>
                      </>
                    )}
                  </div>
                );
              })}
            </div>

            {/* Archived runtimes (older than latest release) */}
            {archivedMr.length > 0 && (
              <>
                <button className="flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-300 mt-2"
                  onClick={() => setShowArchived(!showArchived)}>
                  <Archive size={12} />
                  {showArchived ? "Hide" : "Show"} {archivedMr.length} archived
                  {showArchived ? <ChevronUp size={11} /> : <ChevronDown size={11} />}
                </button>
                {showArchived && (
                  <div className="space-y-1.5 mt-2">
                    {archivedMr.map((r) => (
                      <div key={r.build}
                        className="flex items-center gap-3 px-3 py-2 border border-border">
                        <span className="text-sm font-mono text-gray-400">b{r.build}</span>
                        <span className="text-xs text-gray-500 uppercase">{r.backend_label}</span>
                        <span className="text-xs text-gray-600 ml-auto">
                          {new Date(r.installed_at * 1000).toLocaleDateString()}
                        </span>
                        <button className="btn-ghost text-xs py-0.5 px-1.5"
                          onClick={() => activateManaged(r.build)} title="Activate">
                          <Play size={11} />
                        </button>
                        <button className="text-gray-600 hover:text-accent-red"
                          onClick={() => deleteManaged(r.build)} title="Delete">
                          <Trash2 size={12} />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>

      {/* ── Custom Runtimes ── */}
      <div className="card">
        <div className="flex items-center justify-between mb-3">
          <h2 className="section-title mb-0">Custom Runtimes</h2>
          <button className="btn-ghost text-xs" onClick={browseCustom}>
            <FolderOpen size={12} /> Add
          </button>
        </div>
        {custom.length === 0 ? (
          <p className="text-sm text-gray-500">No custom runtimes. Point to a local llama.cpp installation.</p>
        ) : (
          <div className="space-y-1.5">
            {custom.map((c, i) => {
              const isActive = activeCustomIdx === i;
              return (
                <div key={i}
                  className={`flex items-center gap-3 px-3 py-2 border transition-colors ${
                    isActive ? "border-primary/40 bg-primary/5" : "border-border"
                  }`}>
                  <span className="text-sm text-gray-200">{c.label}</span>
                  <span className="text-xs text-gray-500 font-mono truncate flex-1">{c.binary_path}</span>
                  {isActive ? (
                    <span className="badge-purple text-[10px]">Active</span>
                  ) : (
                    <>
                      <button className="btn-ghost text-xs py-0.5 px-1.5" onClick={() => activateCustom(i)}>
                        <Play size={11} />
                      </button>
                      <button className="text-gray-600 hover:text-accent-red" onClick={() => removeCustom(i)}>
                        <Trash2 size={12} />
                      </button>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* ── Available Backends ── */}
      <div className="card">
        <h2 className="section-title">Available Backends</h2>
        <p className="section-desc">Backends detected on this system.</p>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          {backends.map((b) => (
            <div key={b.id}
              className={`flex items-start gap-2 p-3 border ${
                b.available ? "border-accent-green/30 bg-accent-green/5" : "border-border bg-surface-3 opacity-50"
              }`}>
              <Zap size={13} className={b.available ? "text-accent-green mt-0.5" : "text-gray-600 mt-0.5"} />
              <div>
                <p className="text-xs font-medium text-gray-200">{b.name}</p>
                <p className="text-xs text-gray-500 mt-0.5">{b.description}</p>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* ── GitHub Releases (toggleable) ── */}
      {showReleases && (
        <div className="card">
          <div className="flex items-center justify-between mb-3">
            <h2 className="section-title mb-0">Download Runtime</h2>
            <div className="flex gap-2">
              <button className="btn-secondary text-xs" onClick={checkUpdate} disabled={checking}>
                <RefreshCw size={13} className={checking ? "animate-spin" : ""} />
                Refresh
              </button>
              <button className="btn-ghost text-xs" onClick={() => setShowReleases(false)}>Close</button>
            </div>
          </div>

          {release ? (
            <>
              <div className="flex items-center gap-3 mb-4">
                <Package size={14} className="text-gray-400" />
                <span className="text-sm text-gray-300">
                  Latest: <span className="font-mono text-gray-100">{release.tag_name}</span>
                </span>
                <span className="text-xs text-gray-600 ml-auto">
                  {new Date(release.published_at).toLocaleDateString()}
                </span>
              </div>

              <div className="space-y-1.5">
                {displayedAssets.map((asset) => (
                  <AssetRow key={asset.name} asset={asset}
                    selected={selectedAsset === asset.name}
                    onSelect={() => setSelectedAsset(asset.name)} />
                ))}
              </div>

              {(release.available_assets.length > 5) && (
                <button className="btn-ghost text-xs mt-2 w-full justify-center"
                  onClick={() => setShowAll(!showAll)}>
                  {showAll
                    ? <><ChevronUp size={13} /> Show fewer</>
                    : <><ChevronDown size={13} /> Show all {release.available_assets.length} assets</>}
                </button>
              )}

              {selectedAsset && (
                <div className="mt-4">
                  {downloading ? (
                    <div>
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-sm text-gray-300">
                          {progress?.status === "extracting" ? "Extracting…" : "Downloading…"}
                        </span>
                        <div className="flex items-center gap-3">
                          <span className="text-xs font-mono text-gray-400">{(progress?.percent ?? 0).toFixed(1)}%</span>
                          {progress?.status !== "extracting" && (
                            <button className="btn-ghost text-xs text-accent-red py-0.5" onClick={cancelDownload}>
                              Cancel
                            </button>
                          )}
                        </div>
                      </div>
                      <div className="progress-bar">
                        <div className="progress-fill" style={{ width: `${progress?.percent ?? 0}%` }} />
                      </div>
                    </div>
                  ) : (
                    <button className="btn-primary w-full justify-center" onClick={startDownload}>
                      <Download size={15} />
                      Download {selectedAsset}
                    </button>
                  )}
                </div>
              )}
            </>
          ) : checking ? (
            <p className="text-sm text-gray-500">Fetching releases…</p>
          ) : (
            <p className="text-sm text-gray-500">Click Refresh to check for available releases.</p>
          )}
        </div>
      )}
    </div>
  );
}

function AssetRow({ asset, selected, onSelect }: {
  asset: AssetOption; selected: boolean; onSelect: () => void;
}) {
  return (
    <button
      className={`w-full flex items-center gap-3 px-3 py-2.5 border text-left transition-colors ${
        selected ? "border-primary/60 bg-primary/10" : "border-border hover:border-border-strong hover:bg-surface-3"
      }`}
      onClick={onSelect}>
      <div className={`w-3 h-3 rounded-full border-2 shrink-0 ${selected ? "border-primary bg-primary" : "border-gray-600"}`} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-gray-200 truncate">{asset.name}</span>
          {asset.score >= 90 && <span className="badge-green text-[10px] shrink-0">Recommended</span>}
          {asset.score >= 60 && asset.score < 90 && (
            <span className="badge-purple text-[10px] shrink-0">{asset.backend_label}</span>
          )}
        </div>
        <div className="flex gap-3 mt-0.5">
          <span className="text-xs text-gray-500">{asset.backend_label}</span>
          <span className="text-xs text-gray-600">{mbToStr(asset.size_mb)}</span>
        </div>
      </div>
    </button>
  );
}
