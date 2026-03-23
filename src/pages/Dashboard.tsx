import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import {
  Cpu,
  MemoryStick,
  Monitor,
  Zap,
  AlertCircle,
  ChevronRight,
  HardDrive,
  Play,
  Square,
  Star,
  Trash2,
  CheckCircle,
} from "lucide-react";
import type {
  SystemInfo,
  RuntimeInfo,
  ModelInfo,
  ServerStatus,
  ServerConfig,
  AppConfig,
} from "../types";

import { mbToGb, shortCpuName, shortGpuName, quantColor } from "../utils/format";

function StatusDot({ ok }: { ok: boolean }) {
  return (
    <span
      className={`inline-block w-2 h-2 rounded-full ${ok ? "bg-accent-green" : "bg-accent-red"}`}
    />
  );
}

export default function Dashboard() {
  const navigate = useNavigate();
  const [system, setSystem] = useState<SystemInfo | null>(null);
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [serverStatus, setServerStatus] = useState<ServerStatus>({
    type: "stopped",
  });
  const [appConfig, setAppConfig] = useState<AppConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [launching, setLaunching] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const loadData = async () => {
    const [sys, rt, mdls, srv, cfg] = await Promise.all([
      invoke<SystemInfo>("get_system_info").catch(() => null),
      invoke<RuntimeInfo>("get_runtime_info").catch(() => null),
      invoke<ModelInfo[]>("list_installed_models").catch(() => []),
      invoke<ServerStatus>("get_server_status").catch(() => ({
        type: "stopped" as const,
      })),
      invoke<AppConfig>("get_config").catch(() => null),
    ]);
    setSystem(sys);
    setRuntime(rt);
    setModels(mdls);
    setServerStatus(srv);
    setAppConfig(cfg);
    setLoading(false);
  };

  useEffect(() => {
    loadData();
    const interval = setInterval(async () => {
      try {
        const s = await invoke<ServerStatus>("get_server_status");
        setServerStatus(s);
      } catch {}
    }, 2000);
    return () => clearInterval(interval);
  }, []);

  const selectedModel = models.find(
    (m) => m.path === appConfig?.selected_model
  );
  const launchModel = selectedModel ?? models[0];

  const quickLaunch = async () => {
    if (!launchModel || !runtime?.installed) return;
    setLaunching(true);
    setError(null);
    try {
      const suggested = await invoke<ServerConfig>("suggest_server_config", {
        modelPath: launchModel.path,
        modelSizeMb: Math.round(launchModel.size_bytes / (1024 * 1024)),
      });
      await invoke("start_server", { config: suggested });
    } catch (e) {
      setError(String(e));
    } finally {
      setLaunching(false);
    }
  };

  const quickStop = async () => {
    setStopping(true);
    try {
      await invoke("stop_server");
      setServerStatus({ type: "stopped" });
    } catch (e) {
      setError(String(e));
    } finally {
      setStopping(false);
    }
  };

  const toggleFavorite = async (modelId: string) => {
    try {
      await invoke("toggle_favorite_model", { modelId });
      const cfg = await invoke<AppConfig>("get_config");
      setAppConfig(cfg);
    } catch {}
  };

  const selectForServer = async (modelPath: string) => {
    try {
      await invoke("set_selected_model", { modelPath });
      const cfg = await invoke<AppConfig>("get_config");
      setAppConfig(cfg);
    } catch {}
  };

  const deleteModel = async (path: string) => {
    try {
      await invoke("delete_model", { path });
      setConfirmDelete(null);
      await loadData();
    } catch (e) {
      setError(String(e));
    }
  };

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-500">
        Loading system info…
      </div>
    );
  }

  const totalVram = system?.gpus.reduce((s, g) => s + g.vram_mb, 0) ?? 0;
  const totalUsable = totalVram > 0
    ? totalVram + (system?.available_ram_mb ?? 0)
    : system?.available_ram_mb ?? 0;

  const sizeAdvice = (() => {
    const gb = totalUsable / 1024;
    if (gb >= 40) return "70B+ models (Q4_K_M)";
    if (gb >= 16) return "13B–30B models (Q4_K_M)";
    if (gb >= 8) return "7B–13B models (Q4_K_M)";
    if (gb >= 4) return "3B–7B models (Q4_K_M)";
    return "1B–3B models (Q4_K_M)";
  })();

  const favorites = appConfig?.favorite_models ?? [];
  const isRunning = serverStatus.type === "running" || serverStatus.type === "starting";

  const favoriteModels = models
    .filter((m) => favorites.includes(m.id))
    .sort((a, b) => a.name.localeCompare(b.name));

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-6">
      {error && (
        <div className="card border-accent-red/30 bg-accent-red/5">
          <p className="text-sm text-accent-red">{error}</p>
        </div>
      )}

      {/* System info cards */}
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <div className="card flex items-start gap-3">
          <div className="w-8 h-8 bg-accent-blue/15 flex items-center justify-center shrink-0">
            <Cpu size={16} className="text-accent-blue" />
          </div>
          <div className="min-w-0">
            <p className="text-xs text-gray-500 mb-0.5">CPU</p>
            <p className="text-sm font-medium text-gray-200 truncate">
              {shortCpuName(system?.cpu_name ?? "Unknown")}
            </p>
            <p className="text-xs text-gray-500">
              {system?.cpu_cores}c / {system?.cpu_threads}t
            </p>
          </div>
        </div>

        <div className="card flex items-start gap-3">
          <div className="w-8 h-8 bg-accent-cyan/15 flex items-center justify-center shrink-0">
            <MemoryStick size={16} className="text-accent-cyan" />
          </div>
          <div>
            <p className="text-xs text-gray-500 mb-0.5">RAM</p>
            <p className="text-sm font-medium text-gray-200">
              {mbToGb(system?.total_ram_mb ?? 0)} total
            </p>
            <p className="text-xs text-gray-500">
              {mbToGb(system?.available_ram_mb ?? 0)} free
            </p>
          </div>
        </div>

        <div className="card flex items-start gap-3">
          <div className="w-8 h-8 bg-primary/20 flex items-center justify-center shrink-0">
            <Monitor size={16} className="text-primary-light" />
          </div>
          <div className="min-w-0">
            <p className="text-xs text-gray-500 mb-0.5">GPU</p>
            {system?.gpus && system.gpus.length > 0 ? (
              <>
                <p className="text-sm font-medium text-gray-200 truncate">
                  {shortGpuName(system.gpus[0].name)}
                </p>
                {system.gpus.length > 1 && (
                  <p className="text-xs text-gray-400 truncate">
                    {shortGpuName(system.gpus[1].name)}
                    {system.gpus.length > 2 && (
                      <span className="text-gray-500">
                        {" "}+{system.gpus.length - 2} more
                      </span>
                    )}
                  </p>
                )}
                <p className="text-xs text-gray-500">
                  {totalVram > 0 ? mbToGb(totalVram) + " VRAM" : "shared memory"}
                </p>
              </>
            ) : (
              <p className="text-sm text-gray-500">No GPU detected</p>
            )}
          </div>
        </div>

        <div className="card flex items-start gap-3">
          <div className="w-8 h-8 bg-accent-green/15 flex items-center justify-center shrink-0">
            <Zap size={16} className="text-accent-green" />
          </div>
          <div>
            <p className="text-xs text-gray-500 mb-0.5">Best Backend</p>
            <p className="text-sm font-medium text-gray-200 uppercase">
              {system?.recommended_backend ?? "CPU"}
            </p>
            <p className="text-xs text-gray-500">{sizeAdvice}</p>
          </div>
        </div>
      </div>

      {/* Quick launch bar */}
      <div className="card">
        <div className="flex items-center gap-4">
          <div className="flex-1 min-w-0">
            {isRunning ? (
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-accent-green animate-pulse" />
                <span className="text-sm text-gray-200">
                  Server {serverStatus.type === "starting" ? "starting…" : "running"}
                </span>
                {serverStatus.type === "running" && (
                  <span className="text-xs text-gray-500 font-mono">
                    :{serverStatus.port}
                  </span>
                )}
              </div>
            ) : launchModel ? (
              <div>
                <p className="text-sm text-gray-300 truncate">
                  {launchModel.name}
                  {launchModel.quant && (
                    <span className="text-xs text-gray-500 ml-2">
                      {launchModel.quant}
                    </span>
                  )}
                </p>
                <p className="text-xs text-gray-500">
                  {selectedModel ? "Selected model" : "First available model"}
                </p>
              </div>
            ) : (
              <p className="text-sm text-gray-500">
                No models installed
              </p>
            )}
          </div>
          <div className="flex items-center gap-2">
            {isRunning ? (
              <>
                {serverStatus.type === "running" && (
                  <button
                    className="btn-secondary text-xs"
                    onClick={() => navigate("/chat")}
                  >
                    Chat
                  </button>
                )}
                <button
                  className="btn-danger text-xs"
                  onClick={quickStop}
                  disabled={stopping}
                >
                  <Square size={13} />
                  Stop
                </button>
              </>
            ) : (
              <button
                className="btn-primary text-xs"
                onClick={quickLaunch}
                disabled={!launchModel || !runtime?.installed || launching}
              >
                <Play size={13} />
                {launching ? "Starting…" : "Run"}
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Status row */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <button
          className="card text-left hover:border-border-strong transition-colors group"
          onClick={() => navigate("/runtime")}
        >
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <StatusDot ok={runtime?.installed ?? false} />
              <span className="text-sm font-medium text-gray-300">
                Runtime
              </span>
            </div>
            <ChevronRight
              size={14}
              className="text-gray-600 group-hover:text-gray-400 transition-colors"
            />
          </div>
          {runtime?.installed ? (
            <>
              <p className="text-xs text-gray-400">
                {runtime.runtime_type === "managed" ? (
                  <>Build <span className="text-gray-200 font-mono">b{runtime.build}</span></>
                ) : (
                  <span className="text-gray-200">Custom</span>
                )}
              </p>
              <p className="text-xs text-gray-500 mt-0.5 uppercase">
                {runtime.backend}
              </p>
              {runtime.runtime_type === "managed" && appConfig?.latest_known_build &&
                runtime.build && appConfig.latest_known_build > runtime.build && (
                <p className="text-xs text-accent-yellow mt-1">
                  Update: b{appConfig.latest_known_build}
                </p>
              )}
            </>
          ) : (
            <p className="text-sm text-accent-yellow">
              Not installed — click to download
            </p>
          )}
        </button>

        <button
          className="card text-left hover:border-border-strong transition-colors group"
          onClick={() => navigate("/models")}
        >
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <StatusDot ok={models.length > 0} />
              <span className="text-sm font-medium text-gray-300">Models</span>
            </div>
            <ChevronRight
              size={14}
              className="text-gray-600 group-hover:text-gray-400 transition-colors"
            />
          </div>
          <p className="text-xs text-gray-400">
            <span className="text-gray-200 font-medium">{models.length}</span>{" "}
            model{models.length !== 1 ? "s" : ""} installed
          </p>
          {models.length === 0 && (
            <p className="text-xs text-accent-yellow mt-0.5">
              Download a model to get started
            </p>
          )}
        </button>

        <button
          className="card text-left hover:border-border-strong transition-colors group"
          onClick={() => navigate("/server")}
        >
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <StatusDot ok={serverStatus.type === "running"} />
              <span className="text-sm font-medium text-gray-300">Server</span>
            </div>
            <ChevronRight
              size={14}
              className="text-gray-600 group-hover:text-gray-400 transition-colors"
            />
          </div>
          {serverStatus.type === "running" ? (
            <>
              <p className="text-xs text-accent-green">Running</p>
              <p className="text-xs text-gray-500 font-mono mt-0.5">
                http://127.0.0.1:{serverStatus.port}
              </p>
            </>
          ) : serverStatus.type === "starting" ? (
            <p className="text-xs text-accent-yellow">Starting…</p>
          ) : serverStatus.type === "error" ? (
            <p className="text-xs text-accent-red truncate">
              {serverStatus.message}
            </p>
          ) : (
            <p className="text-xs text-gray-500">Not running</p>
          )}
        </button>
      </div>

      {/* Quick start */}
      {(!runtime?.installed || models.length === 0) && (
        <div className="card border-accent-yellow/30 bg-accent-yellow/5">
          <div className="flex items-start gap-3">
            <AlertCircle size={18} className="text-accent-yellow shrink-0 mt-0.5" />
            <div>
              <p className="text-sm font-medium text-gray-200 mb-1">
                Getting started
              </p>
              <ol className="text-sm text-gray-400 space-y-1 list-decimal list-inside">
                {!runtime?.installed && (
                  <li>
                    <button
                      className="text-primary-light hover:underline"
                      onClick={() => navigate("/runtime")}
                    >
                      Download the runtime
                    </button>{" "}
                    for your hardware
                  </li>
                )}
                {models.length === 0 && (
                  <li>
                    <button
                      className="text-primary-light hover:underline"
                      onClick={() => navigate("/models")}
                    >
                      Download a model
                    </button>{" "}
                    (a 7B Q4_K_M is a good start)
                  </li>
                )}
                <li>
                  <button
                    className="text-primary-light hover:underline"
                    onClick={() => navigate("/server")}
                  >
                    Launch the server
                  </button>{" "}
                  and start chatting
                </li>
              </ol>
            </div>
          </div>
        </div>
      )}

      {/* Favorite models */}
      {favoriteModels.length > 0 && (
        <div>
          <h2 className="text-sm font-semibold text-gray-300 mb-3">
            Favorite Models
          </h2>
          <div className="space-y-1">
            {favoriteModels.map((m) => {
              const isFav = favorites.includes(m.id);
              const isSelected = appConfig?.selected_model === m.path;
              const isDeleting = confirmDelete === m.id;

              return (
                <div
                  key={m.id}
                  className={`flex items-center gap-3 px-3 py-2.5 bg-surface-2 border transition-colors ${
                    isSelected
                      ? "border-primary/50"
                      : "border-border"
                  }`}
                >
                  {/* Favorite toggle */}
                  <button
                    className="shrink-0"
                    onClick={() => toggleFavorite(m.id)}
                    title={isFav ? "Remove from favorites" : "Add to favorites"}
                  >
                    <Star
                      size={14}
                      className={
                        isFav
                          ? "text-accent-yellow fill-accent-yellow"
                          : "text-gray-600 hover:text-gray-400"
                      }
                    />
                  </button>

                  {/* Model info */}
                  <HardDrive size={14} className="text-gray-500 shrink-0" />
                  <span className="flex-1 text-sm text-gray-300 truncate">
                    {m.name}
                  </span>

                  {m.quant && (
                    <span className={`${quantColor(m.quant)} text-[10px]`}>{m.quant}</span>
                  )}
                  <span className="text-xs text-gray-500">
                    {(m.size_bytes / (1024 ** 3)).toFixed(1)} GB
                  </span>

                  {/* Deploy button */}
                  <button
                    className="shrink-0"
                    onClick={() => selectForServer(m.path)}
                    title={isSelected ? "Selected for server" : "Use for server"}
                  >
                    <CheckCircle
                      size={14}
                      className={
                        isSelected
                          ? "text-primary"
                          : "text-gray-600 hover:text-gray-400"
                      }
                    />
                  </button>

                  {/* Delete */}
                  {isDeleting ? (
                    <div className="flex items-center gap-1">
                      <button
                        className="text-xs text-accent-red hover:underline"
                        onClick={() => deleteModel(m.path)}
                      >
                        Confirm
                      </button>
                      <button
                        className="text-xs text-gray-500 hover:underline"
                        onClick={() => setConfirmDelete(null)}
                      >
                        Cancel
                      </button>
                    </div>
                  ) : (
                    <button
                      className="shrink-0"
                      onClick={() => setConfirmDelete(m.id)}
                      title="Delete model"
                    >
                      <Trash2
                        size={14}
                        className="text-gray-600 hover:text-accent-red"
                      />
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
