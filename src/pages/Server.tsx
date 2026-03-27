import { useEffect, useState, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import {
  Play,
  Square,
  ChevronDown,
  ChevronUp,
  HardDrive,
  Terminal,
  ExternalLink,
  Save,
  FolderOpen,
  Trash2,
  Eye,
} from "lucide-react";
import type { ModelInfo, ServerConfig, ServerStatus } from "../types";

// ── Utility components ──────────────────────────────────────────────────────

const KV_TYPES = ["f32", "f16", "bf16", "q8_0", "q4_0", "q4_1", "iq4_nl", "q5_0", "q5_1"] as const;

function Slider({ label, hint, value, min, max, step, onChange, format }: {
  label: string; hint?: string; value: number;
  min: number; max: number; step: number;
  onChange: (v: number) => void; format?: (v: number) => string;
}) {
  return (
    <div>
      <div className="flex justify-between items-baseline mb-1">
        <label className="label mb-0">{label}</label>
        <span className="text-xs font-mono text-gray-300">{format ? format(value) : value}</span>
      </div>
      {hint && <p className="text-xs text-gray-600 mb-1">{hint}</p>}
      <input type="range" min={min} max={max} step={step} value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))} className="w-full accent-primary" />
    </div>
  );
}

function NumberInput({ label, hint, value, min, max, step = 1, onChange }: {
  label: string; hint?: string; value: number | null;
  min?: number; max?: number; step?: number;
  onChange: (v: number | null) => void;
}) {
  const [draft, setDraft] = useState(value != null ? String(value) : "");
  const editing = useRef(false);

  useEffect(() => {
    if (!editing.current) {
      setDraft(value != null ? String(value) : "");
    }
  }, [value]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const raw = e.target.value;
    setDraft(raw);
    if (raw === "") {
      onChange(null);
    } else {
      const n = parseFloat(raw);
      if (!isNaN(n)) onChange(n);
    }
  }, [onChange]);

  const handleBlur = useCallback(() => {
    editing.current = false;
    if (draft === "" || isNaN(parseFloat(draft))) {
      // Restore previous value on blur if empty
      setDraft(value != null ? String(value) : "");
    }
  }, [draft, value]);

  return (
    <div>
      <label className="label">{label}</label>
      {hint && <p className="text-xs text-gray-600 mb-1">{hint}</p>}
      <input type="number" className="input" value={draft} min={min} max={max} step={step}
        onFocus={() => { editing.current = true; }}
        onChange={handleChange}
        onBlur={handleBlur} />
    </div>
  );
}

function Toggle({ label, hint, checked, onChange }: {
  label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start gap-3">
      <button role="switch" aria-checked={checked} onClick={() => onChange(!checked)}
        className={`relative shrink-0 w-8 h-4 rounded-full transition-colors mt-0.5 ${checked ? "bg-primary" : "bg-surface-4"}`}>
        <span className={`absolute top-0.5 left-0.5 w-3 h-3 rounded-full bg-white shadow transition-transform ${checked ? "translate-x-4" : ""}`} />
      </button>
      <div>
        <p className="text-xs font-medium text-gray-300">{label}</p>
        {hint && <p className="text-xs text-gray-600">{hint}</p>}
      </div>
    </div>
  );
}

function TextInput({ label, hint, value, placeholder, onChange }: {
  label: string; hint?: string; value: string; placeholder?: string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <label className="label">{label}</label>
      {hint && <p className="text-xs text-gray-600 mb-1">{hint}</p>}
      <input type="text" className="input" value={value} placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}

function SelectInput({ label, hint, value, options, onChange }: {
  label: string; hint?: string; value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <label className="label">{label}</label>
      {hint && <p className="text-xs text-gray-600 mb-1">{hint}</p>}
      <select className="input" value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
    </div>
  );
}

function Section({ title }: { title: string }) {
  return <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-3 mt-5 first:mt-0">{title}</p>;
}

// ── Tabs ─────────────────────────────────────────────────────────────────────

const TABS = ["Context", "Hardware", "Sampling", "Server", "Chat", "Advanced"] as const;
type Tab = (typeof TABS)[number];

// ── Default config ───────────────────────────────────────────────────────────

const DEFAULT_CONFIG: ServerConfig = {
  model_path: "",
  mmproj_path: null,
  host: "127.0.0.1",
  port: 8080,
  n_ctx: 0,
  n_gpu_layers: -1,
  n_threads: null,
  flash_attn: "auto",
  cache_type_k: "f16",
  cache_type_v: "f16",
  temperature: 0.8,
  top_k: 40,
  min_p: 0.05,
  top_p: 0.95,
  n_predict: -1,
  n_batch: 2048,
  n_ubatch: 512,
  cont_batching: true,
  mlock: false,
  no_mmap: false,
  seed: null,
  rope_freq_scale: null,
  rope_freq_base: null,
  grp_attn_n: null,
  grp_attn_w: null,
  parallel: 1,
  extra_params: {},
};

// ── Main component ───────────────────────────────────────────────────────────

// Session-storage helpers to retain config across page navigation
const SESSION_CONFIG_KEY = "catapult_server_config";
const SESSION_PRESET_KEY = "catapult_server_preset";
const SESSION_TAB_KEY = "catapult_server_tab";
const SESSION_STATUS_KEY = "catapult_server_status";

function loadSessionConfig(): ServerConfig | null {
  try {
    const raw = sessionStorage.getItem(SESSION_CONFIG_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch { return null; }
}

function loadSessionStatus(): ServerStatus {
  try {
    const raw = sessionStorage.getItem(SESSION_STATUS_KEY);
    return raw ? JSON.parse(raw) : { type: "stopped" };
  } catch { return { type: "stopped" }; }
}

export default function Server() {
  const navigate = useNavigate();
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [config, setConfigRaw] = useState<ServerConfig>(() => loadSessionConfig() ?? DEFAULT_CONFIG);
  const [status, setStatusRaw] = useState<ServerStatus>(loadSessionStatus);
  const [logs, setLogs] = useState<string[]>([]);
  const [activeTab, setActiveTabRaw] = useState<Tab>(() => {
    const saved = sessionStorage.getItem(SESSION_TAB_KEY);
    return saved && TABS.includes(saved as Tab) ? (saved as Tab) : "Context";
  });
  const [showLogs, setShowLogs] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [openingChat, setOpeningChat] = useState(false);
  const [showModelList, setShowModelList] = useState(false);
  const logsRef = useRef<HTMLDivElement>(null);
  const pendingLogs = useRef<string[]>([]);
  const logRaf = useRef<number | null>(null);
  const [presets, setPresets] = useState<string[]>([]);
  const [activePreset, setActivePresetRaw] = useState<string | null>(() => {
    return sessionStorage.getItem(SESSION_PRESET_KEY) || null;
  });
  const [showPresetMenu, setShowPresetMenu] = useState(false);
  const [saveName, setSaveName] = useState("");
  const [favorites, setFavorites] = useState<string[]>([]);

  // Wrappers that persist to sessionStorage
  const setConfig: typeof setConfigRaw = useCallback((v) => {
    setConfigRaw((prev) => {
      const next = typeof v === "function" ? v(prev) : v;
      sessionStorage.setItem(SESSION_CONFIG_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const setActivePreset = useCallback((v: string | null) => {
    setActivePresetRaw(v);
    if (v) sessionStorage.setItem(SESSION_PRESET_KEY, v);
    else sessionStorage.removeItem(SESSION_PRESET_KEY);
  }, []);

  const setActiveTab = useCallback((v: Tab) => {
    setActiveTabRaw(v);
    sessionStorage.setItem(SESSION_TAB_KEY, v);
  }, []);

  const setStatus = useCallback((v: ServerStatus | ((prev: ServerStatus) => ServerStatus)) => {
    setStatusRaw((prev) => {
      const next = typeof v === "function" ? v(prev) : v;
      sessionStorage.setItem(SESSION_STATUS_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const flushLogs = useCallback(() => {
    logRaf.current = null;
    const batch = pendingLogs.current;
    if (batch.length === 0) return;
    pendingLogs.current = [];
    setLogs((prev) => [...prev, ...batch].slice(-500));
  }, []);

  const addLog = useCallback((line: string) => {
    pendingLogs.current.push(line);
    if (logRaf.current === null) {
      logRaf.current = requestAnimationFrame(flushLogs);
    }
  }, [flushLogs]);

  useEffect(() => {
    if (logsRef.current) logsRef.current.scrollTop = logsRef.current.scrollHeight;
  }, [logs]);

  // ── Extra params helpers ──────────────────────────────────────────────────

  const getEp = (key: string): string => config.extra_params?.[key] ?? "";
  const getEpNum = (key: string): number | null => {
    const v = config.extra_params?.[key];
    if (v === undefined || v === "") return null;
    const n = parseFloat(v);
    return isNaN(n) ? null : n;
  };
  const setEp = (key: string, value: string) => {
    setConfig((c) => {
      const ep = { ...c.extra_params };
      if (value === "") delete ep[key]; else ep[key] = value;
      return { ...c, extra_params: ep };
    });
  };
  const setEpNum = (key: string, value: number | null) => {
    setEp(key, value !== null ? String(value) : "");
  };
  const hasFlag = (key: string): boolean => key in (config.extra_params ?? {});
  const setFlag = (key: string, on: boolean) => {
    setConfig((c) => {
      const ep = { ...c.extra_params };
      if (on) ep[key] = ""; else delete ep[key];
      return { ...c, extra_params: ep };
    });
  };

  // ── Presets ────────────────────────────────────────────────────────────────

  const refreshPresets = async () => {
    try { setPresets(await invoke<string[]>("list_server_presets")); } catch {}
  };

  const savePreset = async (name: string) => {
    if (!name.trim()) return;
    try {
      // Exclude model_path and mmproj_path from presets — these are per-session
      const presetConfig = { ...config, model_path: "", mmproj_path: null };
      await invoke("save_server_preset", { name: name.trim(), config: presetConfig });
      setActivePreset(name.trim());
      setSaveName("");
      await refreshPresets();
    } catch (e) { setError(String(e)); }
  };

  const loadPreset = async (name: string) => {
    try {
      const loaded = await invoke<ServerConfig>("load_server_preset", { name });
      // Preserve current model_path and mmproj_path
      setConfig((prev) => ({
        ...loaded,
        model_path: prev.model_path,
        mmproj_path: prev.mmproj_path,
      }));
      setActivePreset(name);
      setShowPresetMenu(false);
    } catch (e) { setError(String(e)); }
  };

  const deletePreset = async (name: string) => {
    try {
      await invoke("delete_server_preset", { name });
      if (activePreset === name) setActivePreset(null);
      await refreshPresets();
    } catch (e) { setError(String(e)); }
  };

  const saveAsDefaults = async () => {
    try {
      const presetConfig = { ...config, model_path: "", mmproj_path: null };
      await invoke("save_server_preset", { name: "__default__", config: presetConfig });
      setActivePreset(null);
      setShowPresetMenu(false);
    } catch (e) { setError(String(e)); }
  };

  const resetDefaults = async () => {
    try {
      await invoke("delete_server_preset", { name: "__default__" });
    } catch {}
    const modelPath = config.model_path;
    const mmproj = config.mmproj_path;
    setConfig({ ...DEFAULT_CONFIG, model_path: modelPath, mmproj_path: mmproj });
    setActivePreset(null);
    setShowPresetMenu(false);
  };

  const loadDefaults = async () => {
    try {
      const loaded = await invoke<ServerConfig>("load_server_preset", { name: "__default__" });
      setConfig((prev) => ({
        ...loaded,
        model_path: prev.model_path,
        mmproj_path: prev.mmproj_path,
      }));
    } catch {
      // No saved defaults — use built-in defaults
    }
  };

  const resetToDefault = () => {
    loadDefaults().then(() => {
      setActivePreset(null);
      setShowPresetMenu(false);
    });
  };

  // ── Data loading ──────────────────────────────────────────────────────────

  const openChat = async () => {
    if (status.type !== "running") return;
    setOpeningChat(true);
    try { await invoke("open_chat_window", { port: status.port }); }
    catch (e) { setError(String(e)); }
    finally { setOpeningChat(false); }
  };

  const loadData = async () => {
    const [mdls, srv, cfg] = await Promise.all([
      invoke<ModelInfo[]>("list_installed_models").catch(() => []),
      invoke<ServerStatus>("get_server_status").catch(() => ({ type: "stopped" as const })),
      invoke<{ favorite_models: string[]; selected_model: string | null }>("get_config").catch(() => ({ favorite_models: [], selected_model: null })),
    ]);
    setFavorites(cfg.favorite_models);
    setModels(mdls);
    setStatus(srv);
    if (mdls.length > 0 && !config.model_path) {
      // Use the dashboard-selected model if set, otherwise first model
      const selected = cfg.selected_model
        ? mdls.find((m) => m.path === cfg.selected_model)
        : null;
      const pick = selected ?? mdls[0];
      setConfig((c) => ({
        ...c,
        model_path: pick.path,
        mmproj_path: pick.is_vision && pick.mmproj_path ? pick.mmproj_path : null,
      }));
    }
  };

  useEffect(() => {
    loadData();
    refreshPresets();
    // Only load defaults if no session-restored config
    if (!loadSessionConfig()) loadDefaults();
    // Load any existing logs (e.g. server started from Dashboard)
    invoke<string[]>("get_server_logs").then((existing) => {
      if (existing.length > 0) {
        setLogs(existing);
        setShowLogs(true);
      }
    }).catch(() => {});
    const unlistenLog = listen<string>("server_log", (e) => {
      addLog(e.payload);
    });
    const interval = setInterval(async () => {
      try { setStatus(await invoke<ServerStatus>("get_server_status")); } catch {}
    }, 2000);
    return () => {
      unlistenLog.then((f) => f());
      clearInterval(interval);
      if (logRaf.current !== null) cancelAnimationFrame(logRaf.current);
    };
  }, []);

  const applyModelConfig = async (modelPath: string) => {
    const model = models.find((m) => m.path === modelPath);
    if (!model) return;
    try {
      const suggested = await invoke<ServerConfig>("suggest_server_config", {
        modelPath, modelSizeMb: Math.round(model.size_bytes / (1024 * 1024)),
      });
      // Only apply hardware-dependent suggestions; preserve all user settings
      setConfig((prev) => ({
        ...prev,
        n_ctx: suggested.n_ctx,
        n_gpu_layers: suggested.n_gpu_layers,
      }));
    } catch {}
  };

  const handleModelChange = (m: ModelInfo) => {
    setConfig((c) => ({
      ...c,
      model_path: m.path,
      mmproj_path: m.is_vision && m.mmproj_path ? m.mmproj_path : null,
    }));
    applyModelConfig(m.path);
  };

  const startServer = async () => {
    if (!config.model_path) { setError("Please select a model."); return; }
    setError(null); setLogs([]); setShowLogs(true);
    try { await invoke("start_server", { config }); }
    catch (e) { setError(String(e)); }
  };

  const stopServer = async () => {
    try { await invoke("stop_server"); }
    catch (e) { setError(String(e)); }
  };

  const isRunning = status.type === "running" || status.type === "starting";

  return (
    <div className="flex-1 overflow-hidden flex flex-col">
      {/* Header */}
      <div className="px-6 pt-6 pb-4 border-b border-border flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-100">Run Server</h1>
          <div className="flex items-center gap-2 mt-1">
            {/* Preset controls */}
            <div className="relative">
              <button className="btn-ghost text-xs py-1 px-2" onClick={() => setShowPresetMenu(!showPresetMenu)}>
                <FolderOpen size={12} />
                {activePreset ?? "Default"}
                <ChevronDown size={11} />
              </button>
              {showPresetMenu && (
                <div className="absolute left-0 top-full mt-1 z-50 bg-surface-2 border border-border shadow-lg min-w-[250px]">
                  <button className="w-full text-left px-3 py-2 text-xs text-gray-300 hover:bg-surface-3 flex items-center gap-2"
                    onClick={resetToDefault}>
                    Default
                    {!activePreset && <span className="text-primary-light ml-auto text-[10px]">active</span>}
                  </button>
                  {presets.filter((n) => n !== "__default__").map((name) => (
                    <div key={name} className="flex items-center hover:bg-surface-3 group">
                      <button className="flex-1 text-left px-3 py-2 text-xs text-gray-300 flex items-center gap-2"
                        onClick={() => loadPreset(name)}>
                        {name}
                        {activePreset === name && <span className="text-primary-light ml-auto text-[10px]">active</span>}
                      </button>
                      <button className="px-2 py-2 text-gray-600 hover:text-accent-red opacity-0 group-hover:opacity-100"
                        onClick={(e) => { e.stopPropagation(); deletePreset(name); }}>
                        <Trash2 size={11} />
                      </button>
                    </div>
                  ))}
                  <div className="border-t border-border">
                    <button className="w-full text-left px-3 py-2 text-xs text-gray-400 hover:bg-surface-3 hover:text-gray-200"
                      onClick={saveAsDefaults}>
                      Save current settings as defaults
                    </button>
                    <button className="w-full text-left px-3 py-2 text-xs text-gray-400 hover:bg-surface-3 hover:text-gray-200"
                      onClick={resetDefaults}>
                      Reset defaults to built-in
                    </button>
                  </div>
                  <div className="border-t border-border px-2 py-2 flex gap-1">
                    <input className="input text-xs py-1 flex-1" placeholder="Preset name…"
                      value={saveName} onChange={(e) => setSaveName(e.target.value)}
                      onKeyDown={(e) => { if (e.key === "Enter") savePreset(saveName); }} />
                    <button className="btn-primary text-xs py-1 px-2" onClick={() => savePreset(saveName)}
                      disabled={!saveName.trim()}>
                      <Save size={11} /> Save
                    </button>
                  </div>
                </div>
              )}
            </div>
            {activePreset && (
              <button className="btn-ghost text-xs py-1 px-2" onClick={() => savePreset(activePreset)}
                title="Overwrite current preset">
                <Save size={12} />
              </button>
            )}
          </div>
        </div>
        <div className="flex items-center gap-3">
          {status.type === "running" && (
            <>
              <span className="flex items-center gap-1.5 text-xs text-accent-green">
                <span className="w-1.5 h-1.5 rounded-full bg-accent-green animate-pulse" />
                Running on port {status.port}
              </span>
              <button className="btn-secondary text-xs" onClick={openChat} disabled={openingChat}>
                <ExternalLink size={13} /> Open Chat
              </button>
            </>
          )}
          {status.type === "starting" && (
            <span className="text-xs text-accent-yellow flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-accent-yellow animate-pulse" />
              Starting…
            </span>
          )}
          {status.type === "error" && <span className="text-xs text-accent-red">Error</span>}
          {isRunning ? (
            <button className="btn-danger" onClick={stopServer}><Square size={14} /> Stop</button>
          ) : (
            <button className="btn-primary" onClick={startServer} disabled={!config.model_path}>
              <Play size={14} /> Launch
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6 space-y-5">
        {error && (
          <div className="card border-accent-red/30 bg-accent-red/5">
            <p className="text-sm text-accent-red">{error}</p>
          </div>
        )}

        {/* Model selection */}
        <div className="card">
          {models.length === 0 ? (
            <>
              <h2 className="section-title">Model</h2>
              <p className="text-sm text-gray-500">
                No models installed.{" "}
                <button className="text-primary-light hover:underline" onClick={() => navigate("/models")}>
                  Download one first.
                </button>
              </p>
            </>
          ) : (() => {
            const selected = models.find((m) => m.path === config.model_path);
            return (
              <>
                <button className="w-full flex items-center justify-between"
                  onClick={() => setShowModelList(!showModelList)}>
                  <div className="flex items-center gap-2 min-w-0">
                    <h2 className="section-title mb-0 shrink-0">Model</h2>
                    {selected && !showModelList && (
                      <span className="text-sm text-gray-300 truncate">
                        {selected.name}
                        {selected.quant && <span className="text-gray-500 ml-1">{selected.quant}</span>}
                        {selected.is_vision && selected.mmproj_path && (
                          <Eye size={11} className="inline ml-1.5 text-accent-blue" />
                        )}
                      </span>
                    )}
                  </div>
                  {showModelList
                    ? <ChevronUp size={14} className="text-gray-500 shrink-0" />
                    : <ChevronDown size={14} className="text-gray-500 shrink-0" />}
                </button>
                {showModelList && (
                  <div className="space-y-2 mt-3">
                    {[...models].sort((a, b) => {
                      const aFav = favorites.includes(a.id) ? 0 : 1;
                      const bFav = favorites.includes(b.id) ? 0 : 1;
                      if (aFav !== bFav) return aFav - bFav;
                      return a.name.localeCompare(b.name);
                    }).map((m) => {
                      const isSelected = config.model_path === m.path;
                      const hasVision = m.is_vision && !!m.mmproj_path;
                      return (
                        <button key={m.id}
                          className={`w-full flex items-center gap-3 px-3 py-2.5 border text-left transition-colors ${
                            isSelected ? "border-primary/60 bg-primary/10" : "border-border hover:border-border-strong hover:bg-surface-3"
                          }`}
                          onClick={() => { handleModelChange(m); setShowModelList(false); }}>
                          <div className={`w-3 h-3 rounded-full border-2 shrink-0 ${isSelected ? "border-primary bg-primary" : "border-gray-600"}`} />
                          <HardDrive size={13} className="text-gray-500 shrink-0" />
                          <span className="flex-1 text-sm text-gray-200 truncate">{m.name}</span>
                          {hasVision && (
                            <span className="badge-blue text-[10px]" title={`Vision: ${m.mmproj_path}`}>
                              <Eye size={9} className="mr-0.5" /> Vision
                            </span>
                          )}
                          {m.is_vision && !m.mmproj_path && (
                            <span className="badge-gray text-[10px]" title="Vision model but no mmproj file found">
                              <Eye size={9} className="mr-0.5 opacity-50" /> No mmproj
                            </span>
                          )}
                          {m.quant && <span className="badge-purple text-[10px]">{m.quant}</span>}
                          <span className="text-xs text-gray-500">{(m.size_bytes / 1024 ** 3).toFixed(1)} GB</span>
                        </button>
                      );
                    })}
                  </div>
                )}
              </>
            );
          })()}
        </div>

        {/* Tab bar */}
        <div className="flex border-b border-border -mb-3">
          {TABS.map((tab) => (
            <button key={tab}
              className={`px-4 py-2 text-xs font-medium transition-colors ${
                activeTab === tab ? "text-primary-light border-b-2 border-primary" : "text-gray-500 hover:text-gray-300"
              }`}
              onClick={() => setActiveTab(tab)}>
              {tab}
            </button>
          ))}
        </div>

        {/* Tab content */}
        <div className="card space-y-4">

          {/* ════════════════════════ CONTEXT ════════════════════════ */}
          {activeTab === "Context" && <>
            <Section title="Context & Prediction" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="Context Size" hint="0 = auto (model default)" value={config.n_ctx} min={0} max={1048576} step={512}
                onChange={(v) => setConfig((c) => ({ ...c, n_ctx: v ?? 0 }))} />
              <NumberInput label="Max Tokens" hint="-1 = unlimited" value={config.n_predict} min={-1}
                onChange={(v) => setConfig((c) => ({ ...c, n_predict: v ?? -1 }))} />
              <NumberInput label="Batch Size" hint="Logical max batch (default: 2048)" value={config.n_batch} min={1} max={16384} step={32}
                onChange={(v) => setConfig((c) => ({ ...c, n_batch: v ?? 2048 }))} />
              <NumberInput label="Micro-batch Size" hint="Physical max batch (default: 512)" value={config.n_ubatch} min={1} max={16384} step={32}
                onChange={(v) => setConfig((c) => ({ ...c, n_ubatch: v ?? 512 }))} />
              <NumberInput label="Keep Tokens" hint="Tokens to keep from initial prompt (0=none, -1=all)" value={getEpNum("keep")}
                onChange={(v) => setEpNum("keep", v)} />
            </div>

            <Section title="Attention & KV Cache" />
            <div className="grid grid-cols-2 gap-3">
              <SelectInput label="Flash Attention" value={config.flash_attn}
                options={[{ value: "auto", label: "Auto" }, { value: "on", label: "On" }, { value: "off", label: "Off" }]}
                onChange={(v) => setConfig((c) => ({ ...c, flash_attn: v }))} />
              <div /> {/* spacer */}
              <SelectInput label="KV Cache Type (K)" value={config.cache_type_k}
                options={KV_TYPES.map((t) => ({ value: t, label: t }))}
                onChange={(v) => setConfig((c) => ({ ...c, cache_type_k: v }))} />
              <SelectInput label="KV Cache Type (V)" value={config.cache_type_v}
                options={KV_TYPES.map((t) => ({ value: t, label: t }))}
                onChange={(v) => setConfig((c) => ({ ...c, cache_type_v: v }))} />
            </div>
            <div className="space-y-3 mt-2">
              <Toggle label="SWA Full" hint="Use full-size sliding window attention cache" checked={hasFlag("swa-full")} onChange={(v) => setFlag("swa-full", v)} />
              <Toggle label="KV Offload" hint="Offload KV cache to GPU (default: on)" checked={!hasFlag("no-kv-offload")} onChange={(v) => setFlag("no-kv-offload", !v)} />
              <Toggle label="KV Unified" hint="Single unified KV buffer shared across sequences" checked={hasFlag("kv-unified") || (!hasFlag("no-kv-unified") && config.parallel <= 1)}
                onChange={(v) => { setFlag("kv-unified", v); setFlag("no-kv-unified", !v); }} />
              <Toggle label="Context Shift" hint="Use context shift on infinite text generation" checked={hasFlag("context-shift")} onChange={(v) => { setFlag("context-shift", v); setFlag("no-context-shift", !v); }} />
              <Toggle label="Cache Prompt" hint="Enable prompt caching (default: on)" checked={!hasFlag("no-cache-prompt")} onChange={(v) => setFlag("no-cache-prompt", !v)} />
            </div>
            <div className="grid grid-cols-2 gap-3 mt-2">
              <NumberInput label="Cache Reuse" hint="Min chunk for KV shifting reuse (0=disabled)" value={getEpNum("cache-reuse")} min={0}
                onChange={(v) => setEpNum("cache-reuse", v)} />
              <NumberInput label="Cache RAM (MiB)" hint="Max cache size (-1=no limit, 0=disabled, default: 8192)" value={getEpNum("cache-ram")}
                onChange={(v) => setEpNum("cache-ram", v)} />
              <NumberInput label="Context Checkpoints" hint="Max checkpoints per slot (default: 32)" value={getEpNum("ctx-checkpoints")} min={0}
                onChange={(v) => setEpNum("ctx-checkpoints", v)} />
              <NumberInput label="Checkpoint Interval" hint="Checkpoint every N tokens (-1=disable, default: 8192)" value={getEpNum("checkpoint-every-n-tokens")}
                onChange={(v) => setEpNum("checkpoint-every-n-tokens", v)} />
            </div>
          </>}

          {/* ════════════════════════ HARDWARE ════════════════════════ */}
          {activeTab === "Hardware" && <>
            <Section title="GPU" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="GPU Layers" hint="-1 = all on GPU, 0 = CPU only" value={config.n_gpu_layers} min={-1}
                onChange={(v) => setConfig((c) => ({ ...c, n_gpu_layers: v ?? -1 }))} />
              <SelectInput label="Split Mode" hint="Multi-GPU split strategy" value={getEp("split-mode") || "layer"}
                options={[{ value: "none", label: "None (single GPU)" }, { value: "layer", label: "Layer (default)" }, { value: "row", label: "Row" }]}
                onChange={(v) => setEp("split-mode", v === "layer" ? "" : v)} />
              <TextInput label="Tensor Split" hint="GPU split ratios, e.g. 3,1" value={getEp("tensor-split")} placeholder="e.g. 3,1"
                onChange={(v) => setEp("tensor-split", v)} />
              <NumberInput label="Main GPU" hint="Primary GPU index (default: 0)" value={getEpNum("main-gpu")} min={0}
                onChange={(v) => setEpNum("main-gpu", v)} />
              <TextInput label="Device" hint="Devices for offloading, comma-separated" value={getEp("device")}
                onChange={(v) => setEp("device", v)} />
              <SelectInput label="Fit" hint="Auto-adjust params to fit device memory" value={getEp("fit") || "on"}
                options={[{ value: "on", label: "On (default)" }, { value: "off", label: "Off" }]}
                onChange={(v) => setEp("fit", v === "on" ? "" : v)} />
              <TextInput label="Fit Target (MiB)" hint="Target margin per device (default: 1024)" value={getEp("fit-target")} placeholder="1024"
                onChange={(v) => setEp("fit-target", v)} />
              <NumberInput label="Fit Min Ctx" hint="Min context size for --fit (default: 4096)" value={getEpNum("fit-ctx")} min={0}
                onChange={(v) => setEpNum("fit-ctx", v)} />
            </div>

            <Section title="CPU" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="Threads" hint="CPU threads for generation (-1=auto)" value={config.n_threads}
                onChange={(v) => setConfig((c) => ({ ...c, n_threads: v }))} />
              <NumberInput label="Threads Batch" hint="Threads for batch processing (default: same as threads)" value={getEpNum("threads-batch")}
                onChange={(v) => setEpNum("threads-batch", v)} />
              <SelectInput label="NUMA" hint="NUMA optimizations" value={getEp("numa") || ""}
                options={[{ value: "", label: "Disabled" }, { value: "distribute", label: "Distribute" }, { value: "isolate", label: "Isolate" }, { value: "numactl", label: "numactl" }]}
                onChange={(v) => setEp("numa", v)} />
            </div>

            <Section title="Memory" />
            <div className="space-y-3">
              <Toggle label="mlock" hint="Lock model in RAM (prevents swapping)" checked={config.mlock}
                onChange={(v) => setConfig((c) => ({ ...c, mlock: v }))} />
              <Toggle label="Memory Map" hint="Memory-map model file (default: on)" checked={!config.no_mmap}
                onChange={(v) => setConfig((c) => ({ ...c, no_mmap: !v }))} />
              <Toggle label="Direct IO" hint="Use DirectIO if available" checked={hasFlag("direct-io")} onChange={(v) => setFlag("direct-io", v)} />
              <Toggle label="CPU MoE" hint="Keep all MoE weights on CPU" checked={hasFlag("cpu-moe")} onChange={(v) => setFlag("cpu-moe", v)} />
              <Toggle label="Repack" hint="Enable weight repacking (default: on)" checked={!hasFlag("no-repack")} onChange={(v) => setFlag("no-repack", !v)} />
              <Toggle label="Op Offload" hint="Offload host tensor ops to device (default: on)" checked={!hasFlag("no-op-offload")} onChange={(v) => setFlag("no-op-offload", !v)} />
              <Toggle label="No Host Buffer" hint="Bypass host buffer for extra device buffers" checked={hasFlag("no-host")} onChange={(v) => setFlag("no-host", v)} />
              <Toggle label="Check Tensors" hint="Validate model tensor data on load" checked={hasFlag("check-tensors")} onChange={(v) => setFlag("check-tensors", v)} />
            </div>
            <div className="grid grid-cols-2 gap-3 mt-2">
              <NumberInput label="N CPU MoE Layers" hint="Keep MoE weights of first N layers on CPU" value={getEpNum("n-cpu-moe")} min={0}
                onChange={(v) => setEpNum("n-cpu-moe", v)} />
            </div>

            <Section title="Overrides" />
            <div className="grid grid-cols-1 gap-3">
              <TextInput label="Override Tensor" hint="<pattern>=<buffer type>,... e.g. attn_v=cuda0" value={getEp("override-tensor")}
                onChange={(v) => setEp("override-tensor", v)} />
              <TextInput label="Override KV" hint="KEY=TYPE:VALUE,... e.g. tokenizer.ggml.add_bos_token=bool:false" value={getEp("override-kv")}
                onChange={(v) => setEp("override-kv", v)} />
            </div>
          </>}

          {/* ════════════════════════ SAMPLING ════════════════════════ */}
          {activeTab === "Sampling" && <>
            <Section title="Basic" />
            <Slider label="Temperature" hint="Higher = more creative" value={config.temperature} min={0} max={2} step={0.01}
              onChange={(v) => setConfig((c) => ({ ...c, temperature: v }))} format={(v) => v.toFixed(2)} />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="Seed" hint="Empty or -1 = random" value={config.seed !== null ? config.seed : -1}
                onChange={(v) => setConfig((c) => ({ ...c, seed: v !== null && v >= 0 ? v : null }))} />
              <TextInput label="Samplers" hint="Sampler chain, semicolon-separated" value={getEp("samplers")}
                placeholder="penalties;dry;top_n_sigma;top_k;typ_p;top_p;min_p;xtc;temperature"
                onChange={(v) => setEp("samplers", v)} />
            </div>

            <Section title="Nucleus / Top-K / Min-P" />
            <Slider label="Top-K" value={config.top_k} min={0} max={200} step={1}
              onChange={(v) => setConfig((c) => ({ ...c, top_k: v }))} />
            <Slider label="Top-P" value={config.top_p} min={0} max={1} step={0.01}
              onChange={(v) => setConfig((c) => ({ ...c, top_p: v }))} format={(v) => v.toFixed(2)} />
            <Slider label="Min-P" hint="Minimum probability relative to top token" value={config.min_p} min={0} max={1} step={0.001}
              onChange={(v) => setConfig((c) => ({ ...c, min_p: v }))} format={(v) => v.toFixed(3)} />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="Top-N-Sigma" hint="-1 = disabled" value={getEpNum("top-n-sigma")}
                onChange={(v) => setEpNum("top-n-sigma", v)} />
              <NumberInput label="Typical P" hint="1.0 = disabled (default)" value={getEpNum("typical")}
                onChange={(v) => setEpNum("typical", v)} />
            </div>

            <Section title="Penalties" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="Repeat Last N" hint="0=disabled, -1=ctx_size (default: 64)" value={getEpNum("repeat-last-n")}
                onChange={(v) => setEpNum("repeat-last-n", v)} />
              <NumberInput label="Repeat Penalty" hint="1.0 = disabled (default)" value={getEpNum("repeat-penalty")} step={0.01}
                onChange={(v) => setEpNum("repeat-penalty", v)} />
              <NumberInput label="Presence Penalty" hint="0.0 = disabled (default)" value={getEpNum("presence-penalty")} step={0.01}
                onChange={(v) => setEpNum("presence-penalty", v)} />
              <NumberInput label="Frequency Penalty" hint="0.0 = disabled (default)" value={getEpNum("frequency-penalty")} step={0.01}
                onChange={(v) => setEpNum("frequency-penalty", v)} />
            </div>

            <Section title="XTC" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="XTC Probability" hint="0.0 = disabled (default)" value={getEpNum("xtc-probability")} step={0.01}
                onChange={(v) => setEpNum("xtc-probability", v)} />
              <NumberInput label="XTC Threshold" hint="1.0 = disabled (default: 0.10)" value={getEpNum("xtc-threshold")} step={0.01}
                onChange={(v) => setEpNum("xtc-threshold", v)} />
            </div>

            <Section title="DRY Sampling" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="DRY Multiplier" hint="0.0 = disabled (default)" value={getEpNum("dry-multiplier")} step={0.1}
                onChange={(v) => setEpNum("dry-multiplier", v)} />
              <NumberInput label="DRY Base" hint="Default: 1.75" value={getEpNum("dry-base")} step={0.05}
                onChange={(v) => setEpNum("dry-base", v)} />
              <NumberInput label="DRY Allowed Length" hint="Default: 2" value={getEpNum("dry-allowed-length")} min={0}
                onChange={(v) => setEpNum("dry-allowed-length", v)} />
              <NumberInput label="DRY Penalty Last N" hint="-1=ctx size, 0=disable" value={getEpNum("dry-penalty-last-n")}
                onChange={(v) => setEpNum("dry-penalty-last-n", v)} />
            </div>

            <Section title="Adaptive Sampling" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="Adaptive Target" hint="Target probability (negative=disabled, default: -1)" value={getEpNum("adaptive-target")} step={0.01}
                onChange={(v) => setEpNum("adaptive-target", v)} />
              <NumberInput label="Adaptive Decay" hint="Decay rate (0.0-0.99, default: 0.90)" value={getEpNum("adaptive-decay")} step={0.01}
                onChange={(v) => setEpNum("adaptive-decay", v)} />
            </div>

            <Section title="Dynamic Temperature" />
            <div className="grid grid-cols-2 gap-3">
              <NumberInput label="Range" hint="0.0 = disabled (default)" value={getEpNum("dynatemp-range")} step={0.1}
                onChange={(v) => setEpNum("dynatemp-range", v)} />
              <NumberInput label="Exponent" hint="Default: 1.0" value={getEpNum("dynatemp-exp")} step={0.1}
                onChange={(v) => setEpNum("dynatemp-exp", v)} />
            </div>

            <Section title="Mirostat" />
            <div className="grid grid-cols-2 gap-3">
              <SelectInput label="Mode" value={getEp("mirostat") || "0"}
                options={[{ value: "0", label: "Disabled" }, { value: "1", label: "Mirostat 1" }, { value: "2", label: "Mirostat 2" }]}
                onChange={(v) => setEp("mirostat", v === "0" ? "" : v)} />
              <NumberInput label="Learning Rate" hint="Default: 0.10" value={getEpNum("mirostat-lr")} step={0.01}
                onChange={(v) => setEpNum("mirostat-lr", v)} />
              <NumberInput label="Target Entropy" hint="Default: 5.00" value={getEpNum("mirostat-ent")} step={0.1}
                onChange={(v) => setEpNum("mirostat-ent", v)} />
            </div>

            <Section title="Misc" />
            <div className="space-y-3">
              <Toggle label="Ignore EOS" hint="Continue generating past end-of-stream" checked={hasFlag("ignore-eos")} onChange={(v) => setFlag("ignore-eos", v)} />
              <Toggle label="Backend Sampling" hint="Experimental backend sampling" checked={hasFlag("backend-sampling")} onChange={(v) => setFlag("backend-sampling", v)} />
            </div>
          </>}

          {/* ════════════════════════ SERVER ════════════════════════ */}
          {activeTab === "Server" && <>
            <Section title="Network" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="Host" value={config.host} onChange={(v) => setConfig((c) => ({ ...c, host: v }))} />
              <NumberInput label="Port" value={config.port} min={1} max={65535}
                onChange={(v) => setConfig((c) => ({ ...c, port: v ?? 8080 }))} />
              <NumberInput label="Parallel Slots" hint="-1 = auto (default)" value={config.parallel} min={-1} max={128}
                onChange={(v) => setConfig((c) => ({ ...c, parallel: v ?? 1 }))} />
              <NumberInput label="Timeout (s)" hint="Read/write timeout (default: 600)" value={getEpNum("timeout")} min={0}
                onChange={(v) => setEpNum("timeout", v)} />
              <NumberInput label="HTTP Threads" hint="-1 = auto (default)" value={getEpNum("threads-http")}
                onChange={(v) => setEpNum("threads-http", v)} />
              <NumberInput label="Sleep Idle (s)" hint="Sleep after N seconds idle (-1=disabled)" value={getEpNum("sleep-idle-seconds")}
                onChange={(v) => setEpNum("sleep-idle-seconds", v)} />
            </div>

            <Section title="API" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="API Key" hint="Comma-separated keys for auth" value={getEp("api-key")}
                onChange={(v) => setEp("api-key", v)} />
              <TextInput label="API Key File" hint="Path to file with API keys" value={getEp("api-key-file")}
                onChange={(v) => setEp("api-key-file", v)} />
              <TextInput label="Alias" hint="Model name aliases for API" value={getEp("alias")}
                onChange={(v) => setEp("alias", v)} />
              <TextInput label="Tags" hint="Model tags (informational)" value={getEp("tags")}
                onChange={(v) => setEp("tags", v)} />
              <TextInput label="API Prefix" hint="URL prefix without trailing slash" value={getEp("api-prefix")}
                onChange={(v) => setEp("api-prefix", v)} />
              <NumberInput label="Slot Prompt Similarity" hint="Min prompt match for slot reuse (0=disabled, default: 0.10)" value={getEpNum("slot-prompt-similarity")} step={0.01}
                onChange={(v) => setEpNum("slot-prompt-similarity", v)} />
            </div>

            <Section title="Features" />
            <div className="space-y-3">
              <Toggle label="Continuous Batching" hint="Process multiple requests simultaneously (default: on)" checked={config.cont_batching}
                onChange={(v) => setConfig((c) => ({ ...c, cont_batching: v }))} />
              <Toggle label="WebUI" hint="Serve built-in web interface (default: on)" checked={!hasFlag("no-webui")} onChange={(v) => setFlag("no-webui", !v)} />
              <Toggle label="WebUI MCP Proxy" hint="Experimental MCP CORS proxy" checked={hasFlag("webui-mcp-proxy")} onChange={(v) => setFlag("webui-mcp-proxy", v)} />
              <Toggle label="Metrics" hint="Prometheus-compatible metrics endpoint" checked={hasFlag("metrics")} onChange={(v) => setFlag("metrics", v)} />
              <Toggle label="Props" hint="Allow changing global properties via POST /props" checked={hasFlag("props")} onChange={(v) => setFlag("props", v)} />
              <Toggle label="Slots Endpoint" hint="Expose slot monitoring (default: on)" checked={!hasFlag("no-slots")} onChange={(v) => setFlag("no-slots", !v)} />
              <Toggle label="Embedding" hint="Restrict to embedding-only mode" checked={hasFlag("embedding")} onChange={(v) => setFlag("embedding", v)} />
              <Toggle label="Reranking" hint="Enable reranking endpoint" checked={hasFlag("reranking")} onChange={(v) => setFlag("reranking", v)} />
              <Toggle label="Warmup" hint="Perform warmup run on start (default: on)" checked={!hasFlag("no-warmup")} onChange={(v) => setFlag("no-warmup", !v)} />
            </div>

            <Section title="SSL" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="SSL Key File" value={getEp("ssl-key-file")} onChange={(v) => setEp("ssl-key-file", v)} />
              <TextInput label="SSL Cert File" value={getEp("ssl-cert-file")} onChange={(v) => setEp("ssl-cert-file", v)} />
            </div>

            <Section title="Paths" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="Static Files Path" value={getEp("path")} onChange={(v) => setEp("path", v)} />
              <TextInput label="Slot Save Path" hint="Path to save slot KV cache" value={getEp("slot-save-path")}
                onChange={(v) => setEp("slot-save-path", v)} />
              <TextInput label="Media Path" hint="Directory for local media files" value={getEp("media-path")}
                onChange={(v) => setEp("media-path", v)} />
              <TextInput label="WebUI Config" hint="JSON overriding WebUI defaults" value={getEp("webui-config")}
                onChange={(v) => setEp("webui-config", v)} />
            </div>
          </>}

          {/* ════════════════════════ CHAT ════════════════════════ */}
          {activeTab === "Chat" && <>
            <Section title="Chat Template" />
            <div className="grid grid-cols-1 gap-3">
              <TextInput label="Chat Template" hint="Jinja template name or inline template" value={getEp("chat-template")}
                onChange={(v) => setEp("chat-template", v)} />
              <TextInput label="Chat Template File" hint="Path to Jinja template file" value={getEp("chat-template-file")}
                onChange={(v) => setEp("chat-template-file", v)} />
              <TextInput label="Chat Template Kwargs" hint="JSON object for template params" value={getEp("chat-template-kwargs")}
                placeholder='{"key":"value"}' onChange={(v) => setEp("chat-template-kwargs", v)} />
            </div>
            <div className="space-y-3 mt-2">
              <Toggle label="Jinja" hint="Use Jinja template engine (default: on)" checked={!hasFlag("no-jinja")} onChange={(v) => setFlag("no-jinja", !v)} />
              <Toggle label="Prefill Assistant" hint="Prefill assistant response if last message is assistant (default: on)"
                checked={!hasFlag("no-prefill-assistant")} onChange={(v) => setFlag("no-prefill-assistant", !v)} />
              <Toggle label="Skip Chat Parsing" hint="Force pure content parser, skip tool/reasoning extraction"
                checked={hasFlag("skip-chat-parsing")} onChange={(v) => setFlag("skip-chat-parsing", v)} />
            </div>

            <Section title="Reasoning" />
            <div className="grid grid-cols-2 gap-3">
              <SelectInput label="Reasoning" hint="Enable thinking" value={getEp("reasoning") || "auto"}
                options={[{ value: "auto", label: "Auto (default)" }, { value: "on", label: "On" }, { value: "off", label: "Off" }]}
                onChange={(v) => setEp("reasoning", v === "auto" ? "" : v)} />
              <SelectInput label="Reasoning Format" value={getEp("reasoning-format") || "auto"}
                options={[{ value: "auto", label: "Auto (default)" }, { value: "none", label: "None" }, { value: "deepseek", label: "DeepSeek" }, { value: "deepseek-legacy", label: "DeepSeek Legacy" }]}
                onChange={(v) => setEp("reasoning-format", v === "auto" ? "" : v)} />
              <NumberInput label="Reasoning Budget" hint="-1=unrestricted (default), 0=immediate end" value={getEpNum("reasoning-budget")}
                onChange={(v) => setEpNum("reasoning-budget", v)} />
              <TextInput label="Budget Message" hint="Message injected when budget exhausted" value={getEp("reasoning-budget-message")}
                onChange={(v) => setEp("reasoning-budget-message", v)} />
            </div>

            <Section title="Output" />
            <div className="space-y-3">
              <Toggle label="Escape Sequences" hint="Process \\n, \\t etc (default: on)" checked={!hasFlag("no-escape")} onChange={(v) => setFlag("no-escape", !v)} />
              <Toggle label="Special Tokens" hint="Output special tokens" checked={hasFlag("special")} onChange={(v) => setFlag("special", v)} />
              <Toggle label="Verbose Prompt" hint="Print verbose prompt before generation" checked={hasFlag("verbose-prompt")} onChange={(v) => setFlag("verbose-prompt", v)} />
              <Toggle label="SPM Infill" hint="Use Suffix/Prefix/Middle infill pattern" checked={hasFlag("spm-infill")} onChange={(v) => setFlag("spm-infill", v)} />
            </div>
            <div className="grid grid-cols-2 gap-3 mt-2">
              <SelectInput label="Pooling" hint="Embedding pooling type" value={getEp("pooling") || ""}
                options={[{ value: "", label: "Model default" }, { value: "none", label: "None" }, { value: "mean", label: "Mean" }, { value: "cls", label: "CLS" }, { value: "last", label: "Last" }, { value: "rank", label: "Rank" }]}
                onChange={(v) => setEp("pooling", v)} />
            </div>
          </>}

          {/* ════════════════════════ ADVANCED ════════════════════════ */}
          {activeTab === "Advanced" && <>
            <Section title="RoPE" />
            <div className="grid grid-cols-2 gap-3">
              <SelectInput label="RoPE Scaling" value={getEp("rope-scaling") || ""}
                options={[{ value: "", label: "Default" }, { value: "none", label: "None" }, { value: "linear", label: "Linear" }, { value: "yarn", label: "YaRN" }]}
                onChange={(v) => setEp("rope-scaling", v)} />
              <NumberInput label="RoPE Scale" hint="Context scaling factor" value={getEpNum("rope-scale")} step={0.1}
                onChange={(v) => setEpNum("rope-scale", v)} />
              <NumberInput label="RoPE Freq Base" value={config.rope_freq_base} step={1000}
                onChange={(v) => setConfig((c) => ({ ...c, rope_freq_base: v }))} />
              <NumberInput label="RoPE Freq Scale" value={config.rope_freq_scale} step={0.001}
                onChange={(v) => setConfig((c) => ({ ...c, rope_freq_scale: v }))} />
            </div>
            <div className="grid grid-cols-2 gap-3 mt-2">
              <NumberInput label="YaRN Orig Ctx" value={getEpNum("yarn-orig-ctx")} onChange={(v) => setEpNum("yarn-orig-ctx", v)} />
              <NumberInput label="YaRN Ext Factor" hint="-1 default, 0=full interp" value={getEpNum("yarn-ext-factor")} step={0.1}
                onChange={(v) => setEpNum("yarn-ext-factor", v)} />
              <NumberInput label="YaRN Attn Factor" value={getEpNum("yarn-attn-factor")} step={0.1}
                onChange={(v) => setEpNum("yarn-attn-factor", v)} />
              <NumberInput label="YaRN Beta Slow" value={getEpNum("yarn-beta-slow")} step={0.1}
                onChange={(v) => setEpNum("yarn-beta-slow", v)} />
              <NumberInput label="YaRN Beta Fast" value={getEpNum("yarn-beta-fast")} step={0.1}
                onChange={(v) => setEpNum("yarn-beta-fast", v)} />
            </div>

            <Section title="Speculative Decoding" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="Draft Model" hint="Path to draft model for speculation" value={getEp("model-draft")}
                onChange={(v) => setEp("model-draft", v)} />
              <SelectInput label="Spec Type" hint="Without draft model" value={getEp("spec-type") || ""}
                options={[{ value: "", label: "None" }, { value: "ngram-cache", label: "N-gram Cache" }, { value: "ngram-simple", label: "N-gram Simple" },
                  { value: "ngram-map-k", label: "N-gram Map K" }, { value: "ngram-map-k4v", label: "N-gram Map K4V" }, { value: "ngram-mod", label: "N-gram Mod" }]}
                onChange={(v) => setEp("spec-type", v)} />
              <NumberInput label="Draft Max" hint="Max draft tokens (default: 16)" value={getEpNum("draft")} min={1}
                onChange={(v) => setEpNum("draft", v)} />
              <NumberInput label="Draft Min" hint="Min draft tokens (default: 0)" value={getEpNum("draft-min")} min={0}
                onChange={(v) => setEpNum("draft-min", v)} />
              <NumberInput label="Draft P Min" hint="Min probability for greedy (default: 0.75)" value={getEpNum("draft-p-min")} step={0.01}
                onChange={(v) => setEpNum("draft-p-min", v)} />
              <NumberInput label="Draft Ctx Size" hint="0 = from model" value={getEpNum("ctx-size-draft")} min={0}
                onChange={(v) => setEpNum("ctx-size-draft", v)} />
              <NumberInput label="Draft GPU Layers" value={getEpNum("n-gpu-layers-draft")}
                onChange={(v) => setEpNum("n-gpu-layers-draft", v)} />
              <NumberInput label="Draft Threads" value={getEpNum("threads-draft")}
                onChange={(v) => setEpNum("threads-draft", v)} />
            </div>

            <Section title="LoRA & Control Vectors" />
            <div className="grid grid-cols-1 gap-3">
              <TextInput label="LoRA" hint="Comma-separated adapter paths" value={getEp("lora")}
                onChange={(v) => setEp("lora", v)} />
              <TextInput label="LoRA Scaled" hint="FNAME:SCALE,... format" value={getEp("lora-scaled")}
                onChange={(v) => setEp("lora-scaled", v)} />
              <TextInput label="Control Vector" hint="Comma-separated paths" value={getEp("control-vector")}
                onChange={(v) => setEp("control-vector", v)} />
              <TextInput label="Control Vector Scaled" hint="FNAME:SCALE,... format" value={getEp("control-vector-scaled")}
                onChange={(v) => setEp("control-vector-scaled", v)} />
            </div>
            <Toggle label="LoRA Init Without Apply" hint="Load adapters without applying (use POST /lora-adapters later)"
              checked={hasFlag("lora-init-without-apply")} onChange={(v) => setFlag("lora-init-without-apply", v)} />

            <Section title="Multimodal" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="mmproj" hint={config.mmproj_path ? `Auto-detected: ${config.mmproj_path}` : "Multimodal projector file (auto-detected for vision models)"}
                value={getEp("mmproj")} placeholder={config.mmproj_path ?? ""}
                onChange={(v) => setEp("mmproj", v)} />
              <TextInput label="mmproj URL" value={getEp("mmproj-url")}
                onChange={(v) => setEp("mmproj-url", v)} />
              <NumberInput label="Image Min Tokens" value={getEpNum("image-min-tokens")} min={0}
                onChange={(v) => setEpNum("image-min-tokens", v)} />
              <NumberInput label="Image Max Tokens" value={getEpNum("image-max-tokens")} min={0}
                onChange={(v) => setEpNum("image-max-tokens", v)} />
            </div>
            <div className="space-y-3 mt-2">
              <Toggle label="mmproj Offload" hint="GPU offload for multimodal projector (default: on)"
                checked={!hasFlag("no-mmproj-offload")} onChange={(v) => setFlag("no-mmproj-offload", !v)} />
              <Toggle label="mmproj Auto" hint="Auto-use multimodal projector if available (default: on)"
                checked={!hasFlag("no-mmproj")} onChange={(v) => setFlag("no-mmproj", !v)} />
            </div>

            <Section title="CPU Affinity" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="CPU Mask" hint="Hex affinity mask" value={getEp("cpu-mask")} onChange={(v) => setEp("cpu-mask", v)} />
              <TextInput label="CPU Range" hint="lo-hi range" value={getEp("cpu-range")} onChange={(v) => setEp("cpu-range", v)} />
              <SelectInput label="CPU Strict" value={getEp("cpu-strict") || "0"}
                options={[{ value: "0", label: "Off (default)" }, { value: "1", label: "On" }]}
                onChange={(v) => setEp("cpu-strict", v === "0" ? "" : v)} />
              <SelectInput label="Priority" value={getEp("prio") || "0"}
                options={[{ value: "-1", label: "Low" }, { value: "0", label: "Normal (default)" }, { value: "1", label: "Medium" }, { value: "2", label: "High" }, { value: "3", label: "Realtime" }]}
                onChange={(v) => setEp("prio", v === "0" ? "" : v)} />
              <NumberInput label="Poll" hint="Polling level 0-100 (default: 50)" value={getEpNum("poll")} min={0} max={100}
                onChange={(v) => setEpNum("poll", v)} />
            </div>

            <Section title="Logging" />
            <div className="grid grid-cols-2 gap-3">
              <TextInput label="Log File" value={getEp("log-file")} onChange={(v) => setEp("log-file", v)} />
              <SelectInput label="Log Colors" value={getEp("log-colors") || "auto"}
                options={[{ value: "auto", label: "Auto" }, { value: "on", label: "On" }, { value: "off", label: "Off" }]}
                onChange={(v) => setEp("log-colors", v === "auto" ? "" : v)} />
              <SelectInput label="Verbosity" value={getEp("log-verbosity") || "3"}
                options={[{ value: "0", label: "0 - Generic" }, { value: "1", label: "1 - Error" }, { value: "2", label: "2 - Warning" }, { value: "3", label: "3 - Info (default)" }, { value: "4", label: "4 - Debug" }]}
                onChange={(v) => setEp("log-verbosity", v === "3" ? "" : v)} />
            </div>
            <div className="space-y-3 mt-2">
              <Toggle label="Verbose" hint="Log all messages" checked={hasFlag("verbose")} onChange={(v) => setFlag("verbose", v)} />
              <Toggle label="Perf Timings" hint="Internal libllama performance timings" checked={hasFlag("perf")} onChange={(v) => setFlag("perf", v)} />
              <Toggle label="Log Prefix" hint="Add prefix to log messages" checked={hasFlag("log-prefix")} onChange={(v) => setFlag("log-prefix", v)} />
              <Toggle label="Log Timestamps" hint="Add timestamps to log messages" checked={hasFlag("log-timestamps")} onChange={(v) => setFlag("log-timestamps", v)} />
              <Toggle label="Offline" hint="Prevent network access, use cache only" checked={hasFlag("offline")} onChange={(v) => setFlag("offline", v)} />
            </div>

            <Section title="Extra Arguments" />
            <div>
              <label className="label">Raw CLI Arguments</label>
              <p className="text-xs text-gray-600 mb-1">
                Any additional flags not covered above, space-separated
              </p>
              <input type="text" className="input font-mono text-xs"
                value={getEp("__raw__")} placeholder="e.g. --reverse-prompt '### Human:'"
                onChange={(e) => setEp("__raw__", e.target.value)} />
            </div>
          </>}
        </div>

        {/* Server logs */}
        <div className="card">
          <button className="w-full flex items-center justify-between" onClick={() => setShowLogs(!showLogs)}>
            <div className="flex items-center gap-2">
              <Terminal size={15} className="text-gray-400" />
              <span className="text-sm font-medium text-gray-300">Server Logs</span>
              {logs.length > 0 && <span className="badge-gray text-[10px]">{logs.length}</span>}
            </div>
            {showLogs ? <ChevronUp size={14} className="text-gray-500" /> : <ChevronDown size={14} className="text-gray-500" />}
          </button>
          {showLogs && (
            <div ref={logsRef} className="mt-3 bg-surface-0 p-3 h-48 overflow-y-auto font-mono text-xs text-gray-400 space-y-0.5">
              {logs.length === 0 ? (
                <span className="text-gray-600">No logs yet…</span>
              ) : logs.map((line, i) => (
                <div key={i} className={
                  line.toLowerCase().includes("error") ? "text-accent-red" :
                  line.toLowerCase().includes("warn") ? "text-accent-yellow" :
                  line.includes("[stderr]") ? "text-gray-600" : "text-gray-400"
                }>{line}</div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
