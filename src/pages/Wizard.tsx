import { useEffect, useState } from "react";
import { flushSync } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useNavigate } from "react-router-dom";
import {
  Cpu,
  MemoryStick,
  Monitor,
  Zap,
  Download,
  CheckCircle,
  FolderOpen,
  RefreshCw,
  ChevronRight,
  ChevronLeft,
} from "lucide-react";
import CatapultIcon from "../components/CatapultIcon";
import type {
  SystemInfo,
  RuntimeInfo,
  ReleaseInfo,
  AssetOption,
  RecommendedModel,
  DownloadProgress,
  CustomBuild,
  ScanResult,
} from "../types";

function mbToGb(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}

function shortCpuName(raw: string): string {
  const m =
    raw.match(/i[3579]-\w+/) ||
    raw.match(/Ryzen \d+ \w+/) ||
    raw.match(/Core Ultra \d+ \w+/) ||
    raw.match(/Xeon \w[\w-]*/) ||
    raw.match(/EPYC \w+/) ||
    raw.match(/Apple M\d\w*/);
  return m
    ? m[0]
    : raw
        .replace(/\s+CPU.*/, "")
        .replace(/\(R\)|\(TM\)/g, "")
        .trim();
}

function shortGpuName(raw: string): string {
  const m =
    raw.match(/RTX \w+(\s*Ti)?(\s*SUPER)?/) ||
    raw.match(/GTX \w+(\s*Ti)?/) ||
    raw.match(/RX \w+(\s*XT)?(\s*X)?/) ||
    raw.match(/Arc \w+/) ||
    raw.match(/Apple M\d\w*/) ||
    raw.match(/Radeon Pro \w+/) ||
    raw.match(/A\d{3,4}\b/);
  return m ? m[0] : raw.replace(/NVIDIA |GeForce |AMD |Intel /g, "").trim();
}

type FitLevel = "vram" | "mixed" | "tight" | "no";

function modelFit(sizeMb: number, totalVram: number, totalRam: number): FitLevel {
  const totalMem = totalVram + totalRam;
  if (totalVram > 0 && sizeMb < totalVram * 0.85) return "vram";
  if (sizeMb < totalMem * 0.7) return "mixed";
  if (sizeMb < totalMem * 0.9) return "tight";
  return "no";
}

const FIT_LABELS: Record<FitLevel, { text: string; cls: string }> = {
  vram: { text: "Fits in VRAM", cls: "badge-green" },
  mixed: { text: "VRAM + RAM", cls: "badge-blue" },
  tight: { text: "Tight fit", cls: "badge-yellow" },
  no: { text: "Too large", cls: "badge-red" },
};

export default function Wizard() {
  const navigate = useNavigate();
  const [step, setStep] = useState(1);

  // Step 1 state
  const [system, setSystem] = useState<SystemInfo | null>(null);
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const [release, setRelease] = useState<ReleaseInfo | null>(null);
  const [selectedAsset, setSelectedAsset] = useState<string | null>(null);
  const [runtimeProgress, setRuntimeProgress] = useState<DownloadProgress | null>(null);
  const [runtimeDone, setRuntimeDone] = useState(false);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [customBuilds, setCustomBuilds] = useState<CustomBuild[] | null>(null);

  // Step 2 state
  const [recommended, setRecommended] = useState<RecommendedModel[]>([]);
  const [selectedModels, setSelectedModels] = useState<Set<string>>(new Set());
  const [modelProgress, setModelProgress] = useState<Record<string, DownloadProgress>>({});
  const [modelsDone, setModelsDone] = useState<Set<string>>(new Set());
  const [modelsError, setModelsError] = useState<Record<string, string>>({});
  const [downloading, setDownloading] = useState(false);

  // Load system info + runtime status + release info on mount
  useEffect(() => {
    const load = async () => {
      const [sys, rt] = await Promise.all([
        invoke<SystemInfo>("get_system_info").catch(() => null),
        invoke<RuntimeInfo>("get_runtime_info").catch(() => null),
      ]);
      setSystem(sys);
      setRuntime(rt);
      if (rt?.installed) setRuntimeDone(true);

      try {
        const rel = await invoke<ReleaseInfo>("check_latest_release");
        setRelease(rel);
        if (rel.available_assets.length > 0) {
          setSelectedAsset(rel.available_assets[0].name);
        }
      } catch {}

      try {
        const models = await invoke<RecommendedModel[]>("get_recommended_models");
        setRecommended(models);
      } catch {}
    };
    load();

    const unlisten = listen<DownloadProgress>("download_progress", (e) => {
      const p = e.payload;
      if (p.id === "runtime") {
        setRuntimeProgress(p);
        if (p.status === "done") {
          setRuntimeDone(true);
          setRuntimeProgress(null);
          // Reload runtime info
          invoke<RuntimeInfo>("get_runtime_info")
            .then((rt) => setRuntime(rt))
            .catch(() => {});
        }
      } else {
        // Model download
        if (p.status === "done") {
          setModelsDone((prev) => new Set(prev).add(p.id));
          setModelProgress((prev) => {
            const next = { ...prev };
            delete next[p.id];
            return next;
          });
        } else {
          setModelProgress((prev) => ({ ...prev, [p.id]: p }));
        }
      }
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const totalVram = system?.gpus.reduce((s, g) => s + g.vram_mb, 0) ?? 0;
  const totalRam = system?.available_ram_mb ?? 0;

  const finish = async () => {
    await invoke("set_wizard_completed", { completed: true });
    navigate("/dashboard", { replace: true });
  };

  const skip = async () => {
    await invoke("set_wizard_completed", { completed: true });
    navigate("/dashboard", { replace: true });
  };

  // ── Step 1: Runtime ──────────────────────────────────────────

  const downloadRuntime = async () => {
    if (!selectedAsset) return;
    setRuntimeError(null);
    try {
      await invoke("download_runtime", { assetName: selectedAsset });
    } catch (e) {
      setRuntimeError(String(e));
    }
  };

  const browseCustom = async () => {
    const selected = await open({
      directory: true,
      title: "Select llama.cpp directory",
    });
    if (!selected) return;
    flushSync(() => { setScanning(true); setRuntimeError(null); });
    try {
      const result = await invoke<ScanResult>("scan_custom_runtime", {
        path: selected,
      });
      if (result.builds.length === 0) {
        setRuntimeError("No llama-server binary found in the selected directory.");
      } else if (result.is_source_distribution) {
        await invoke("add_all_custom_runtime_binaries", {
          builds: result.builds,
        });
        const rt = await invoke<RuntimeInfo>("get_runtime_info");
        setRuntime(rt);
        setRuntimeDone(true);
      } else if (result.builds.length === 1) {
        await invoke("set_custom_runtime_binary", {
          binaryPath: result.builds[0].binary_path,
        });
        const rt = await invoke<RuntimeInfo>("get_runtime_info");
        setRuntime(rt);
        setRuntimeDone(true);
      } else {
        setCustomBuilds(result.builds);
      }
    } catch (e) {
      setRuntimeError(String(e));
    } finally {
      setScanning(false);
    }
  };

  const selectCustomBuild = async (build: CustomBuild) => {
    try {
      await invoke("set_custom_runtime_binary", {
        binaryPath: build.binary_path,
      });
      const rt = await invoke<RuntimeInfo>("get_runtime_info");
      setRuntime(rt);
      setRuntimeDone(true);
      setCustomBuilds(null);
    } catch (e) {
      setRuntimeError(String(e));
    }
  };

  // ── Step 2: Models ───────────────────────────────────────────

  const toggleModel = (filename: string) => {
    setSelectedModels((prev) => {
      const next = new Set(prev);
      if (next.has(filename)) {
        next.delete(filename);
      } else if (next.size < 3) {
        next.add(filename);
      }
      return next;
    });
  };

  const downloadSelectedModels = async () => {
    setDownloading(true);
    const toDownload = recommended.filter(
      (m) => selectedModels.has(m.filename) && !m.installed && !modelsDone.has(m.filename)
    );

    for (const m of toDownload) {
      try {
        const files = await invoke<{ filename: string; size_bytes: number; download_url: string }[]>(
          "get_hf_repo_files",
          { repoId: m.repo_id }
        );
        const file = files.find((f) => f.filename === m.filename);
        if (!file) {
          setModelsError((prev) => ({ ...prev, [m.filename]: "File not found in repo" }));
          continue;
        }
        // Fire and forget — progress comes via events
        invoke("download_model", {
          repoId: m.repo_id,
          filename: m.filename,
          downloadUrl: file.download_url,
          sizeBytes: file.size_bytes,
        }).catch((e) => {
          setModelsError((prev) => ({ ...prev, [m.filename]: String(e) }));
        });
      } catch (e) {
        setModelsError((prev) => ({ ...prev, [m.filename]: String(e) }));
      }
    }
  };

  const allSelectedDone =
    selectedModels.size > 0 &&
    [...selectedModels].every(
      (f) => modelsDone.has(f) || recommended.find((m) => m.filename === f)?.installed
    );

  // Filter and sort models: fits first, then by size
  const sortedModels = [...recommended].sort((a, b) => {
    const fitOrder: Record<FitLevel, number> = { vram: 0, mixed: 1, tight: 2, no: 3 };
    const aFit = modelFit(a.estimated_size_mb, totalVram, totalRam);
    const bFit = modelFit(b.estimated_size_mb, totalVram, totalRam);
    if (fitOrder[aFit] !== fitOrder[bFit]) return fitOrder[aFit] - fitOrder[bFit];
    return a.estimated_size_mb - b.estimated_size_mb;
  });

  const topAssets = release?.available_assets.slice(0, 5) ?? [];

  return (
    <div className="h-full flex flex-col bg-surface-0">
      {/* Header */}
      <div className="flex items-center justify-between px-8 py-5 border-b border-border">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 bg-primary flex items-center justify-center">
            <CatapultIcon size={16} className="text-white" />
          </div>
          <span className="font-semibold text-gray-100 text-lg">
            Catapult Setup
          </span>
        </div>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2 text-xs text-gray-500">
            <span
              className={`w-6 h-6 flex items-center justify-center border ${
                step === 1
                  ? "border-primary bg-primary/20 text-primary-light"
                  : "border-border text-gray-400"
              }`}
            >
              1
            </span>
            <span className="text-gray-600">—</span>
            <span
              className={`w-6 h-6 flex items-center justify-center border ${
                step === 2
                  ? "border-primary bg-primary/20 text-primary-light"
                  : "border-border text-gray-400"
              }`}
            >
              2
            </span>
          </div>
          <button className="btn-ghost text-xs" onClick={skip}>
            Skip wizard
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-8 py-6">
        {step === 1 ? (
          <div className="max-w-2xl mx-auto space-y-6">
            <div>
              <h2 className="text-xl font-bold text-gray-100">
                System &amp; Runtime
              </h2>
              <p className="text-sm text-gray-500 mt-1">
                We detected your hardware. Choose a llama.cpp build to download
                or point to an existing installation.
              </p>
            </div>

            {scanning && (
              <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
                <div className="card flex items-center gap-3 px-6 py-4">
                  <RefreshCw size={16} className="text-primary animate-spin" />
                  <span className="text-sm text-gray-200">Searching for server runtimes…</span>
                </div>
              </div>
            )}

            {runtimeError && (
              <div className="card border-accent-red/30 bg-accent-red/5">
                <p className="text-sm text-accent-red">{runtimeError}</p>
              </div>
            )}

            {/* System summary */}
            {system && (
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                <div className="card flex items-center gap-2">
                  <Cpu size={14} className="text-accent-blue shrink-0" />
                  <div className="min-w-0">
                    <p className="text-xs text-gray-500">CPU</p>
                    <p className="text-xs font-medium text-gray-200 truncate">
                      {shortCpuName(system.cpu_name)}
                    </p>
                  </div>
                </div>
                <div className="card flex items-center gap-2">
                  <MemoryStick size={14} className="text-accent-cyan shrink-0" />
                  <div>
                    <p className="text-xs text-gray-500">RAM</p>
                    <p className="text-xs font-medium text-gray-200">
                      {mbToGb(system.total_ram_mb)}
                    </p>
                  </div>
                </div>
                <div className="card flex items-center gap-2">
                  <Monitor size={14} className="text-primary-light shrink-0" />
                  <div className="min-w-0">
                    <p className="text-xs text-gray-500">GPU</p>
                    <p className="text-xs font-medium text-gray-200 truncate">
                      {system.gpus.length > 0
                        ? shortGpuName(system.gpus[0].name)
                        : "None"}
                    </p>
                  </div>
                </div>
                <div className="card flex items-center gap-2">
                  <Zap size={14} className="text-accent-green shrink-0" />
                  <div>
                    <p className="text-xs text-gray-500">Backend</p>
                    <p className="text-xs font-medium text-gray-200 uppercase">
                      {system.recommended_backend}
                    </p>
                  </div>
                </div>
              </div>
            )}

            {/* Runtime already installed */}
            {runtimeDone && runtime?.installed && (
              <div className="card border-accent-green/30 bg-accent-green/5">
                <div className="flex items-center gap-2">
                  <CheckCircle size={16} className="text-accent-green" />
                  <span className="text-sm text-gray-200">
                    Runtime ready
                  </span>
                  {runtime.backend && (
                    <span className="badge-green text-[10px] uppercase">
                      {runtime.backend}
                    </span>
                  )}
                </div>
                {runtime.path && (
                  <p className="text-xs text-gray-500 mt-1 font-mono">
                    {runtime.path}
                  </p>
                )}
              </div>
            )}

            {/* Custom build picker */}
            {customBuilds && customBuilds.length > 1 && (
              <div className="card">
                <h3 className="text-sm font-semibold text-gray-200 mb-2">
                  Multiple builds found
                </h3>
                <div className="space-y-1.5">
                  {customBuilds.map((b) => (
                    <button
                      key={b.binary_path}
                      className="w-full flex items-center gap-3 px-3 py-2 border border-border hover:border-border-strong hover:bg-surface-3 text-left transition-colors"
                      onClick={() => selectCustomBuild(b)}
                    >
                      <Zap size={13} className="text-primary-light shrink-0" />
                      <span className="text-xs font-mono text-gray-200 truncate">
                        {b.label}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Download progress */}
            {runtimeProgress && runtimeProgress.status !== "done" && (
              <div className="card">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm text-gray-300">
                    {runtimeProgress.status === "extracting"
                      ? "Extracting…"
                      : "Downloading runtime…"}
                  </span>
                  <span className="text-xs font-mono text-gray-400">
                    {runtimeProgress.percent.toFixed(1)}%
                  </span>
                </div>
                <div className="progress-bar">
                  <div
                    className="progress-fill"
                    style={{ width: `${runtimeProgress.percent}%` }}
                  />
                </div>
              </div>
            )}

            {/* Asset selection (only if runtime not yet installed) */}
            {!runtimeDone && !runtimeProgress && (
              <>
                {topAssets.length > 0 && (
                  <div className="space-y-1.5">
                    {topAssets.map((asset) => (
                      <AssetRow
                        key={asset.name}
                        asset={asset}
                        selected={selectedAsset === asset.name}
                        onSelect={() => setSelectedAsset(asset.name)}
                      />
                    ))}
                  </div>
                )}
                <div className="flex items-center gap-3">
                  {selectedAsset && (
                    <button
                      className="btn-primary text-sm"
                      onClick={downloadRuntime}
                    >
                      <Download size={14} />
                      Download
                    </button>
                  )}
                  <button
                    className="btn-ghost text-sm"
                    onClick={browseCustom}
                  >
                    <FolderOpen size={14} />
                    Use existing installation
                  </button>
                </div>
              </>
            )}
          </div>
        ) : (
          /* ── Step 2: Models ─────────────────────────────── */
          <div className="max-w-2xl mx-auto space-y-6">
            <div>
              <h2 className="text-xl font-bold text-gray-100">
                Choose Models
              </h2>
              <p className="text-sm text-gray-500 mt-1">
                Pick up to 3 models to download. Models are sorted by fit for
                your {totalVram > 0 ? mbToGb(totalVram) + " VRAM + " : ""}
                {mbToGb(totalRam)} RAM.
              </p>
            </div>

            {Object.keys(modelsError).length > 0 && (
              <div className="card border-accent-red/30 bg-accent-red/5">
                {Object.entries(modelsError).map(([f, err]) => (
                  <p key={f} className="text-xs text-accent-red">
                    {f}: {err}
                  </p>
                ))}
              </div>
            )}

            <div className="text-xs text-gray-500">
              {selectedModels.size}/3 selected
            </div>

            <div className="space-y-1.5">
              {sortedModels.map((m) => {
                const fit = modelFit(m.estimated_size_mb, totalVram, totalRam);
                const fitInfo = FIT_LABELS[fit];
                const isSelected = selectedModels.has(m.filename);
                const isDone =
                  m.installed || modelsDone.has(m.filename);
                const prog = modelProgress[m.filename];
                const err = modelsError[m.filename];

                return (
                  <button
                    key={m.filename}
                    className={`w-full text-left px-3 py-3 border transition-colors ${
                      isDone
                        ? "border-accent-green/30 bg-accent-green/5"
                        : isSelected
                        ? "border-primary/60 bg-primary/10"
                        : "border-border hover:border-border-strong hover:bg-surface-3"
                    }`}
                    onClick={() => {
                      if (!isDone && !downloading) toggleModel(m.filename);
                    }}
                    disabled={
                      isDone ||
                      downloading ||
                      (!isSelected && selectedModels.size >= 3 && fit !== "no")
                    }
                  >
                    <div className="flex items-center gap-3">
                      <div
                        className={`w-3.5 h-3.5 border-2 shrink-0 flex items-center justify-center ${
                          isDone
                            ? "border-accent-green bg-accent-green"
                            : isSelected
                            ? "border-primary bg-primary"
                            : "border-gray-600"
                        }`}
                      >
                        {(isDone || isSelected) && (
                          <CheckCircle size={10} className="text-white" />
                        )}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="text-sm font-medium text-gray-200">
                            {m.name}
                          </span>
                          <span className={`text-[10px] ${fitInfo.cls}`}>
                            {fitInfo.text}
                          </span>
                          {isDone && (
                            <span className="badge-green text-[10px]">
                              Installed
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-gray-500 mt-0.5">
                          {m.description}
                        </p>
                        <div className="flex gap-3 mt-0.5 text-xs text-gray-500">
                          <span>{m.params_b}B params</span>
                          <span>{m.quant}</span>
                          <span>~{mbToGb(m.estimated_size_mb)}</span>
                        </div>
                      </div>
                    </div>
                    {prog && (
                      <div className="mt-2">
                        <div className="progress-bar">
                          <div
                            className="progress-fill"
                            style={{ width: `${prog.percent}%` }}
                          />
                        </div>
                        <p className="text-xs text-gray-500 mt-0.5">
                          {prog.percent.toFixed(1)}% —{" "}
                          {mbToGb(prog.bytes_downloaded / (1024 * 1024))} /{" "}
                          {mbToGb(prog.total_bytes / (1024 * 1024))}
                        </p>
                      </div>
                    )}
                    {err && (
                      <p className="text-xs text-accent-red mt-1">{err}</p>
                    )}
                  </button>
                );
              })}
            </div>

            {selectedModels.size > 0 && !downloading && !allSelectedDone && (
              <button
                className="btn-primary text-sm"
                onClick={downloadSelectedModels}
              >
                <Download size={14} />
                Download {selectedModels.size} model
                {selectedModels.size !== 1 ? "s" : ""}
              </button>
            )}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between px-8 py-4 border-t border-border">
        <div>
          {step > 1 && (
            <button
              className="btn-ghost text-sm"
              onClick={() => setStep(step - 1)}
            >
              <ChevronLeft size={14} />
              Back
            </button>
          )}
        </div>
        <div className="flex items-center gap-3">
          {step === 1 ? (
            <button
              className="btn-primary text-sm"
              onClick={() => setStep(2)}
            >
              {runtimeDone ? "Next" : "Skip this step"}
              <ChevronRight size={14} />
            </button>
          ) : (
            <button className="btn-primary text-sm" onClick={finish}>
              {allSelectedDone || downloading
                ? "Finish"
                : selectedModels.size === 0
                ? "Finish without models"
                : "Finish"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function AssetRow({
  asset,
  selected,
  onSelect,
}: {
  asset: AssetOption;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      className={`w-full flex items-center gap-3 px-3 py-2.5 border text-left transition-colors ${
        selected
          ? "border-primary/60 bg-primary/10"
          : "border-border hover:border-border-strong hover:bg-surface-3"
      }`}
      onClick={onSelect}
    >
      <div
        className={`w-3 h-3 rounded-full border-2 shrink-0 ${
          selected ? "border-primary bg-primary" : "border-gray-600"
        }`}
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-gray-200 truncate">
            {asset.name}
          </span>
          {asset.score >= 90 && (
            <span className="badge-green text-[10px] shrink-0">
              Recommended
            </span>
          )}
        </div>
        <div className="flex gap-3 mt-0.5">
          <span className="text-xs text-gray-500">{asset.backend_label}</span>
          <span className="text-xs text-gray-600">{mbToGb(asset.size_mb)}</span>
        </div>
      </div>
    </button>
  );
}
