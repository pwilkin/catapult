import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Search,
  Download,
  Trash2,
  ChevronDown,
  ChevronUp,
  HardDrive,
  CheckCircle,
  FolderPlus,
  FolderOpen,
  Star,
  ArrowUp,
  ArrowDown,
  Filter,
} from "lucide-react";
import type {
  ModelInfo,
  RecommendedModel,
  HfModel,
  HfFile,
  KnownOwner,
  DownloadProgress,
} from "../types";

type Tab = "installed" | "recommended" | "browse" | "settings";
type SortCol = "name" | "params" | "quant" | "size" | "ctx";
type SortDir = "asc" | "desc";


import { formatSize, quantColor, quantSortKey, isImatrixFile } from "../utils/format";
import PreferredOwners from "../components/PreferredOwners";

function QuantBadge({ quant }: { quant: string }) {
  return <span className={`${quantColor(quant)} text-[10px]`}>{quant}</span>;
}

export default function Models() {
  const [tab, setTab] = useState<Tab>("installed");
  const [installed, setInstalled] = useState<ModelInfo[]>([]);
  const [recommended, setRecommended] = useState<RecommendedModel[]>([]);
  const [searchResults, setSearchResults] = useState<HfModel[]>([]);
  const [owners, setOwners] = useState<KnownOwner[]>([]);
  const [selectedOwner, setSelectedOwner] = useState<string>("");
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [downloads, setDownloads] = useState<Record<string, DownloadProgress>>({});
  const [expandedRepo, setExpandedRepo] = useState<string | null>(null);
  const [repoFiles, setRepoFiles] = useState<Record<string, HfFile[]>>({});
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [modelDirs, setModelDirs] = useState<string[]>([]);
  const [downloadDir, setDownloadDir] = useState<string>("");
  const [sortCol, setSortCol] = useState<SortCol>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [explicitSort, setExplicitSort] = useState(false);
  const [nameFilter, setNameFilter] = useState("");
  const [quantFilter, setQuantFilter] = useState("");
  const [favorites, setFavorites] = useState<string[]>([]);
  const [preferredOwners, setPreferredOwners] = useState<string[]>([]);
  const [mmProjPicker, setMmProjPicker] = useState<{
    repoId: string;
    file: HfFile;
    mmProjFiles: HfFile[];
  } | null>(null);

  const reloadDirs = useCallback(async () => {
    try {
      const info = await invoke<{ dirs: string[]; download_dir: string }>("get_model_dirs");
      setModelDirs(info.dirs);
      setDownloadDir(info.download_dir);
    } catch {}
  }, []);

  const reload = useCallback(async () => {
    const [inst, rec] = await Promise.all([
      invoke<ModelInfo[]>("list_installed_models").catch(() => []),
      invoke<RecommendedModel[]>("get_recommended_models").catch(() => []),
    ]);
    setInstalled(inst);
    setRecommended(rec);
  }, []);

  const reloadFavorites = useCallback(async () => {
    try {
      const cfg = await invoke<{ favorite_models: string[] }>("get_config");
      setFavorites(cfg.favorite_models);
    } catch {}
  }, []);

  const toggleFavorite = async (modelId: string) => {
    try {
      await invoke("toggle_favorite_model", { modelId });
      await reloadFavorites();
    } catch {}
  };

  useEffect(() => {
    reload();
    reloadDirs();
    reloadFavorites();
    invoke<KnownOwner[]>("get_known_owners").then(setOwners).catch(() => {});
    invoke<string[]>("get_preferred_owners").then(setPreferredOwners).catch(() => {});

    const unlisten = listen<DownloadProgress>("download_progress", (e) => {
      const p = e.payload;
      setDownloads((prev) => {
        if (p.status === "done") {
          const { [p.id]: _, ...rest } = prev;
          reload();
          return rest;
        }
        return { ...prev, [p.id]: p };
      });
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, [reload]);

  const doSearch = async () => {
    if (!searchQuery.trim() && !selectedOwner) return;
    setSearching(true);
    setError(null);
    try {
      const results = await invoke<HfModel[]>("search_hf_models", {
        query: searchQuery.trim(),
        owner: selectedOwner || null,
      });
      setSearchResults(results);
    } catch (e) {
      setError(String(e));
    } finally {
      setSearching(false);
    }
  };

  const toggleRepo = async (repoId: string) => {
    if (expandedRepo === repoId) {
      setExpandedRepo(null);
      return;
    }
    setExpandedRepo(repoId);
    if (!repoFiles[repoId]) {
      try {
        const files = await invoke<HfFile[]>("get_hf_repo_files", { repoId });
        setRepoFiles((prev) => ({ ...prev, [repoId]: files }));
      } catch (e) {
        setError(String(e));
      }
    }
  };

  const startDownload = async (
    repoId: string,
    file: HfFile,
    companionModel?: string
  ) => {
    // Clear any paused state for this file
    setDownloads((prev) => {
      const { [file.filename]: _, ...rest } = prev;
      return rest;
    });
    try {
      await invoke("download_model", {
        repoId,
        filename: file.filename,
        downloadUrl: file.download_url,
        sizeBytes: file.size_bytes,
        splitParts: file.is_split && file.split_parts.length > 0 ? file.split_parts : null,
        companionModel: companionModel ?? null,
      });
    } catch (e) {
      // Error is expected when retries exhausted — paused status is already set via event
      if (!String(e).includes("failed after")) {
        setError(String(e));
      }
    }
  };

  const handleDownloadClick = (repoId: string, file: HfFile) => {
    const files = repoFiles[repoId] || [];
    const mmProjFiles = files.filter((f) => f.is_mmproj);
    if (mmProjFiles.length > 0 && !file.is_mmproj) {
      setMmProjPicker({ repoId, file, mmProjFiles });
    } else {
      startDownload(repoId, file);
    }
  };

  const handleMmProjChoice = (mmProjFile: HfFile | null) => {
    if (!mmProjPicker) return;
    const { repoId, file } = mmProjPicker;
    setMmProjPicker(null);
    startDownload(repoId, file);
    if (mmProjFile) {
      startDownload(repoId, mmProjFile, file.filename);
    }
  };

  const abortDownload = async (filename: string) => {
    try {
      await invoke("abort_download", { filename });
      setDownloads((prev) => {
        const { [filename]: _, ...rest } = prev;
        return rest;
      });
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteModel = async (m: ModelInfo) => {
    setDeletingId(m.id);
    try {
      await invoke("delete_model", { path: m.path });
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setDeletingId(null);
    }
  };

  const addModelDir = async () => {
    const selected = await open({ directory: true, title: "Add GGUF storage directory" });
    if (selected) {
      try {
        await invoke("add_model_dir", { path: selected });
        await reloadDirs();
        await reload();
      } catch (e) { setError(String(e)); }
    }
  };

  const removeModelDir = async (path: string) => {
    try {
      await invoke("remove_model_dir", { path });
      await reloadDirs();
      await reload();
    } catch (e) { setError(String(e)); }
  };

  const changeDownloadDir = async (path: string) => {
    try {
      await invoke("set_download_dir", { path });
      await reloadDirs();
    } catch (e) { setError(String(e)); }
  };

  const browseNewDownloadDir = async () => {
    const selected = await open({ directory: true, title: "Select default download directory" });
    if (selected) {
      await changeDownloadDir(selected);
      await reload();
    }
  };

  const handlePreferredOwnersChange = async (newOwners: string[]) => {
    setPreferredOwners(newOwners);
    try {
      await invoke("set_preferred_owners", { owners: newOwners });
      const updated = await invoke<KnownOwner[]>("get_known_owners");
      setOwners(updated);
    } catch (e) {
      setError(String(e));
    }
  };

  const tabs: { id: Tab; label: string }[] = [
    { id: "installed", label: `Installed (${installed.length})` },
    { id: "recommended", label: "Recommended" },
    { id: "browse", label: "Browse HuggingFace" },
    { id: "settings", label: "Settings" },
  ];

  return (
    <div className="flex-1 overflow-hidden flex flex-col">
      {/* Header */}
      <div className="px-6 pt-6 pb-4 border-b border-border">
        <h1 className="text-2xl font-bold text-gray-100">Models</h1>
        <p className="text-gray-500 text-sm mt-1">
          Download and manage GGUF models
        </p>
        {/* Tabs */}
        <div className="flex gap-1 mt-4">
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`px-4 py-1.5 text-sm  font-medium transition-colors ${
                tab === t.id
                  ? "bg-surface-3 text-gray-100"
                  : "text-gray-500 hover:text-gray-300"
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {error && (
          <div className="mb-4 card border-accent-red/30 bg-accent-red/5">
            <p className="text-sm text-accent-red">{error}</p>
            <button
              className="text-xs text-accent-red/70 hover:text-accent-red mt-1"
              onClick={() => setError(null)}
            >
              Dismiss
            </button>
          </div>
        )}

        {/* ── Active downloads (visible across all tabs) ── */}
        {Object.keys(downloads).length > 0 && (
          <div className="mb-4 card">
            <h3 className="text-xs font-medium text-gray-400 mb-2">Downloading</h3>
            <div className="space-y-2">
              {Object.entries(downloads).map(([filename, dl]) => (
                <div key={filename}>
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-xs text-gray-300 truncate mr-3">{filename}</span>
                    <div className="flex items-center gap-2 shrink-0">
                      {dl.status === "paused" ? (
                        <>
                          <span className="text-[10px] text-accent-yellow">Paused</span>
                          <button className="btn-danger text-[10px] py-0.5 px-1.5" onClick={() => abortDownload(filename)}>
                            Abort
                          </button>
                        </>
                      ) : (
                        <>
                          <span className="text-[10px] font-mono text-gray-500">{dl.percent.toFixed(1)}%</span>
                          <span className="text-[10px] text-gray-600">{formatSize(dl.bytes_downloaded)} / {formatSize(dl.total_bytes)}</span>
                        </>
                      )}
                    </div>
                  </div>
                  {dl.status !== "paused" && (
                    <div className="progress-bar h-1">
                      <div className="progress-fill" style={{ width: `${dl.percent}%` }} />
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* ── Installed tab ── */}
        {tab === "installed" && (() => {
          const filtered = installed
            .filter((m) => !isImatrixFile(m.filename))
            .filter((m) => !nameFilter || m.name.toLowerCase().includes(nameFilter.toLowerCase()) || m.filename.toLowerCase().includes(nameFilter.toLowerCase()))
            .filter((m) => !quantFilter || (m.quant ?? "").toLowerCase().includes(quantFilter.toLowerCase()));

          const sorted = [...filtered].sort((a, b) => {
            // Favorites first when no explicit column sort
            if (!explicitSort) {
              const aFav = favorites.includes(a.id) ? 0 : 1;
              const bFav = favorites.includes(b.id) ? 0 : 1;
              if (aFav !== bFav) return aFav - bFav;
            }
            let cmp = 0;
            switch (sortCol) {
              case "name": cmp = a.name.localeCompare(b.name); break;
              case "params": cmp = parseFloat(a.params_b ?? "0") - parseFloat(b.params_b ?? "0"); break;
              case "quant": cmp = quantSortKey(a.quant) - quantSortKey(b.quant); break;
              case "size": cmp = a.size_bytes - b.size_bytes; break;
              case "ctx": cmp = (a.context_length ?? 0) - (b.context_length ?? 0); break;
            }
            return sortDir === "asc" ? cmp : -cmp;
          });

          const toggleSort = (col: SortCol) => {
            if (sortCol === col) {
              setSortDir(sortDir === "asc" ? "desc" : "asc");
            } else {
              setSortCol(col);
              setSortDir("asc");
            }
            setExplicitSort(true);
          };

          const SortIcon = ({ col }: { col: SortCol }) => (
            sortCol === col
              ? sortDir === "asc" ? <ArrowUp size={10} /> : <ArrowDown size={10} />
              : <ArrowUp size={10} className="opacity-0 group-hover:opacity-30" />
          );

          return (
            <div>
              {installed.length === 0 ? (
                <div className="card text-center py-12 text-gray-500">
                  <HardDrive size={32} className="mx-auto mb-3 opacity-30" />
                  <p className="text-sm">No models installed yet.</p>
                  <button className="text-sm text-primary-light hover:underline mt-2"
                    onClick={() => setTab("recommended")}>
                    Browse recommended models →
                  </button>
                </div>
              ) : (
                <>
                  {/* Filter row */}
                  <div className="flex items-center gap-3 mb-3">
                    <div className="flex items-center gap-1.5 flex-1">
                      <Filter size={12} className="text-gray-500" />
                      <input className="input text-xs py-1 flex-1" placeholder="Filter by name…"
                        value={nameFilter} onChange={(e) => setNameFilter(e.target.value)} />
                    </div>
                    <input className="input text-xs py-1 w-28" placeholder="Quant…"
                      value={quantFilter} onChange={(e) => setQuantFilter(e.target.value)} />
                    <span className="text-xs text-gray-500">{sorted.length} model{sorted.length !== 1 ? "s" : ""}</span>
                  </div>

                  {/* Table header */}
                  <div className="flex items-center gap-2 px-3 py-2 border-b border-border text-[10px] font-semibold text-gray-500 uppercase tracking-wider select-none">
                    <button className="w-6 flex justify-center" title="Reset to favorites-first"
                      onClick={() => { setSortCol("name"); setSortDir("asc"); setExplicitSort(false); }}>
                      <Star size={10} className={explicitSort ? "text-gray-600" : "text-accent-yellow"} />
                    </button>
                    <button className="flex-1 flex items-center gap-1 group text-left" onClick={() => toggleSort("name")}>
                      Model <SortIcon col="name" />
                    </button>
                    <button className="w-16 flex items-center gap-1 group justify-end" onClick={() => toggleSort("params")}>
                      Params <SortIcon col="params" />
                    </button>
                    <button className="w-20 flex items-center gap-1 group justify-end" onClick={() => toggleSort("quant")}>
                      Quant <SortIcon col="quant" />
                    </button>
                    <button className="w-16 flex items-center gap-1 group justify-end" onClick={() => toggleSort("ctx")}>
                      Ctx <SortIcon col="ctx" />
                    </button>
                    <button className="w-20 flex items-center gap-1 group justify-end" onClick={() => toggleSort("size")}>
                      Size <SortIcon col="size" />
                    </button>
                    <span className="w-8" />
                  </div>

                  {/* Table rows */}
                  <div>
                    {sorted.map((m) => {
                      const isFav = favorites.includes(m.id);
                      return (
                      <div key={m.id}
                        className="flex items-center gap-2 px-3 py-2 border-b border-border/50 hover:bg-surface-3 transition-colors">
                        <button className="w-6 flex justify-center shrink-0"
                          onClick={() => toggleFavorite(m.id)}
                          title={isFav ? "Remove from favorites" : "Add to favorites"}>
                          <Star size={13} className={isFav ? "text-accent-yellow fill-accent-yellow" : "text-gray-600 hover:text-gray-400"} />
                        </button>
                        <div className="flex-1 min-w-0">
                          <p className="text-sm text-gray-200 truncate">{m.name}</p>
                          <p className="text-[10px] text-gray-600 truncate font-mono">{m.filename}</p>
                        </div>
                        <span className="w-16 text-right text-xs text-gray-400">
                          {m.params_b ?? "—"}
                        </span>
                        <span className="w-20 text-right">
                          {m.quant ? <QuantBadge quant={m.quant} /> : <span className="text-xs text-gray-600">—</span>}
                        </span>
                        <span className="w-16 text-right text-xs text-gray-500">
                          {m.context_length ? `${(m.context_length / 1024).toFixed(0)}K` : "—"}
                        </span>
                        <span className="w-20 text-right text-xs text-gray-400 font-mono">
                          {formatSize(m.size_bytes)}
                        </span>
                        <button className="w-8 flex justify-center text-gray-600 hover:text-accent-red"
                          onClick={() => deleteModel(m)} disabled={deletingId === m.id} title="Delete model">
                          <Trash2 size={13} />
                        </button>
                      </div>
                      );
                    })}
                    {sorted.length === 0 && filtered.length === 0 && installed.length > 0 && (
                      <p className="text-sm text-gray-500 py-6 text-center">No models match the filter.</p>
                    )}
                  </div>
                </>
              )}
            </div>
          );
        })()}

        {/* ── Recommended tab ── */}
        {tab === "recommended" && (
          <div className="space-y-3">
            {recommended.map((m) => {
              const dl = downloads[m.filename];
              const isDownloading = !!dl;
              return (
                <div key={m.filename} className="card hover:border-border-strong transition-colors">
                  <div className="flex items-start gap-3">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="text-sm font-semibold text-gray-200">
                          {m.name}
                        </span>
                        <QuantBadge quant={m.quant} />
                        <span className="badge-gray text-[10px]">{m.params_b}B params</span>
                        {m.context != null && (
                          <span className="badge-gray text-[10px]">
                            ctx {(m.context / 1024).toFixed(0)}K
                          </span>
                        )}
                        {m.installed && (
                          <span className="badge-green text-[10px]">
                            <CheckCircle size={9} className="mr-0.5" />
                            Installed
                          </span>
                        )}
                      </div>
                      <p className="text-xs text-gray-400 mt-1">{m.description}</p>
                      <p className="text-xs text-gray-600 mt-0.5 font-mono">
                        {m.repo_id}
                      </p>
                    </div>
                    <div className="shrink-0 flex flex-col items-end gap-2">
                      <span className="text-xs text-gray-500">
                        ~{formatSize(m.estimated_size_mb * 1024 * 1024)}
                      </span>
                      {!m.installed && !isDownloading && (
                        <button
                          className="btn-primary text-xs"
                          onClick={() =>
                            startDownload(m.repo_id, {
                              filename: m.filename,
                              size_bytes: m.estimated_size_mb * 1024 * 1024,
                              quant: m.quant,
                              download_url: `https://huggingface.co/${m.repo_id}/resolve/main/${m.filename}`,
                              is_split: false,
                              split_parts: [],
                              is_mmproj: false,
                            })
                          }
                        >
                          <Download size={12} />
                          Download
                        </button>
                      )}
                      {m.installed && (
                        <span className="text-xs text-accent-green flex items-center gap-1">
                          <CheckCircle size={11} /> Ready
                        </span>
                      )}
                    </div>
                  </div>
                  {isDownloading && (
                    <div className="mt-3">
                      <div className="flex justify-between items-center text-xs text-gray-500 mb-1">
                        <span>
                          {dl.status === "paused" ? "Paused — download failed" :
                           dl.status.startsWith("retrying") ? `Retrying… (${dl.status})` :
                           dl.status === "extracting" ? "Extracting…" : "Downloading…"}
                        </span>
                        <span>{dl.percent.toFixed(1)}%</span>
                      </div>
                      <div className="progress-bar">
                        <div
                          className={dl.status === "paused" ? "progress-fill bg-accent-yellow" : "progress-fill"}
                          style={{ width: `${dl.percent}%` }}
                        />
                      </div>
                      {dl.status === "paused" && (
                        <div className="flex gap-2 mt-2">
                          <button className="btn-primary text-xs" onClick={() =>
                            startDownload(m.repo_id, {
                              filename: m.filename,
                              size_bytes: m.estimated_size_mb * 1024 * 1024,
                              quant: m.quant,
                              download_url: `https://huggingface.co/${m.repo_id}/resolve/main/${m.filename}`,
                              is_split: false,
                              split_parts: [],
                              is_mmproj: false,
                            })
                          }>
                            Resume
                          </button>
                          <button className="btn-danger text-xs" onClick={() => abortDownload(m.filename)}>
                            Abort
                          </button>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {/* ── Browse tab ── */}
        {tab === "browse" && (
          <div className="space-y-4">
            {/* Search bar */}
            <div className="flex gap-2">
              <div className="flex-1 relative">
                <Search
                  size={14}
                  className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-500 pointer-events-none"
                />
                <input
                  className="input pl-9"
                  placeholder="Search models (e.g. llama, mistral, qwen)"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && doSearch()}
                />
              </div>
              <select
                className="input w-48"
                value={selectedOwner}
                onChange={(e) => setSelectedOwner(e.target.value)}
              >
                <option value="">Any owner</option>
                {owners.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.id}
                  </option>
                ))}
              </select>
              <button
                className="btn-primary"
                onClick={doSearch}
                disabled={searching}
              >
                {searching ? "…" : "Search"}
              </button>
            </div>

            {/* Results */}
            {searchResults.length > 0 ? (
              <div className="space-y-2">
                {searchResults.map((model) => (
                  <div key={model.repo_id} className="card">
                    <button
                      className="w-full flex items-start gap-3 text-left"
                      onClick={() => toggleRepo(model.repo_id)}
                    >
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium text-gray-200 truncate">
                            {model.name}
                          </span>
                          <span className="text-xs text-gray-500">
                            by {model.author}
                          </span>
                        </div>
                        <div className="flex gap-3 mt-1 text-xs text-gray-500">
                          <span>↓ {(model.downloads / 1000).toFixed(0)}K</span>
                          <span>♥ {model.likes}</span>
                          <span>{model.files.length} GGUF files</span>
                        </div>
                      </div>
                      {expandedRepo === model.repo_id ? (
                        <ChevronUp size={14} className="text-gray-500 mt-0.5 shrink-0" />
                      ) : (
                        <ChevronDown size={14} className="text-gray-500 mt-0.5 shrink-0" />
                      )}
                    </button>

                    {expandedRepo === model.repo_id && (
                      <div className="mt-3 pt-3 border-t border-border space-y-1.5">
                        {repoFiles[model.repo_id] ? (
                          repoFiles[model.repo_id].filter((f) => !f.is_mmproj).length > 0 ? (
                            repoFiles[model.repo_id].filter((f) => !f.is_mmproj).map((f) => {
                              const dl = downloads[f.filename];
                              const hfBasename = f.filename.includes('/') ? f.filename.split('/').pop()! : f.filename;
                              const isInstalled = installed.some(
                                (m) => m.filename === hfBasename
                              );
                              return (
                                <div
                                  key={f.filename}
                                  className="flex items-center gap-3 px-2 py-2  hover:bg-surface-3"
                                >
                                  <div className="flex-1 min-w-0">
                                    <span className="text-xs text-gray-300 font-mono truncate block">
                                      {f.filename}
                                    </span>
                                    <div className="flex items-center gap-1.5 mt-0.5">
                                      {f.quant && (
                                        <QuantBadge quant={f.quant} />
                                      )}
                                      {f.is_split && (
                                        <span className="badge-gray text-[10px]">
                                          {f.split_parts.length} parts
                                        </span>
                                      )}
                                    </div>
                                  </div>
                                  <span className="text-xs text-gray-500 shrink-0">
                                    {formatSize(f.size_bytes)}
                                  </span>
                                  {isInstalled ? (
                                    <span className="badge-green text-[10px] shrink-0">
                                      Installed
                                    </span>
                                  ) : dl ? (
                                    <div className="shrink-0 flex items-center gap-2">
                                      {dl.status === "paused" ? (
                                        <>
                                          <span className="text-[10px] text-accent-yellow">Paused {dl.percent.toFixed(0)}%</span>
                                          <button className="btn-primary text-[10px] py-0.5 px-1.5"
                                            onClick={() => startDownload(model.repo_id, f)}>
                                            Resume
                                          </button>
                                          <button className="btn-danger text-[10px] py-0.5 px-1.5"
                                            onClick={() => abortDownload(f.filename)}>
                                            Abort
                                          </button>
                                        </>
                                      ) : (
                                        <div className="w-20">
                                          <div className="progress-bar">
                                            <div className="progress-fill" style={{ width: `${dl.percent}%` }} />
                                          </div>
                                          {dl.status.startsWith("retrying") && (
                                            <span className="text-[10px] text-accent-yellow">{dl.status}</span>
                                          )}
                                        </div>
                                      )}
                                    </div>
                                  ) : (
                                    <button
                                      className="btn-secondary text-xs shrink-0"
                                      onClick={() => handleDownloadClick(model.repo_id, f)}
                                    >
                                      <Download size={11} />
                                    </button>
                                  )}
                                </div>
                              );
                            })
                          ) : (
                            <p className="text-xs text-gray-500 px-2">
                              No GGUF files in this repo.
                            </p>
                          )
                        ) : (
                          <p className="text-xs text-gray-500 px-2">
                            Loading files…
                          </p>
                        )}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            ) : !searching ? (
              <div className="card text-center py-12 text-gray-500">
                <Search size={28} className="mx-auto mb-3 opacity-30" />
                <p className="text-sm">
                  Search for GGUF models on HuggingFace.
                </p>
                <p className="text-xs mt-1 text-gray-600">
                  Try "llama 3", "mistral", or select an owner to browse.
                </p>
              </div>
            ) : null}
          </div>
        )}

        {/* ── Settings tab ── */}
        {tab === "settings" && (
          <div className="space-y-6">
            {/* Download directory */}
            <div className="card">
              <h2 className="section-title">Download Directory</h2>
              <p className="text-xs text-gray-500 mb-3">
                New models are downloaded to this directory.
              </p>
              <div className="flex items-center gap-3">
                <span className="flex-1 text-sm font-mono text-gray-300 truncate">
                  {downloadDir}
                </span>
                <button className="btn-ghost text-xs" onClick={browseNewDownloadDir}>
                  <FolderOpen size={12} /> Change
                </button>
              </div>
            </div>

            {/* Scan directories */}
            <div className="card">
              <div className="flex items-center justify-between mb-3">
                <div>
                  <h2 className="section-title">GGUF Storage Directories</h2>
                  <p className="text-xs text-gray-500">
                    All directories below are scanned recursively for .gguf files.
                  </p>
                </div>
                <button className="btn-secondary text-xs" onClick={addModelDir}>
                  <FolderPlus size={13} /> Add Directory
                </button>
              </div>

              <div className="space-y-1.5">
                {modelDirs.map((dir) => {
                  const isDownload = dir === downloadDir;
                  return (
                    <div
                      key={dir}
                      className={`flex items-center gap-3 px-3 py-2.5 border transition-colors ${
                        isDownload
                          ? "border-primary/40 bg-primary/5"
                          : "border-border"
                      }`}
                    >
                      <FolderOpen size={14} className="text-gray-500 shrink-0" />
                      <span className="flex-1 text-sm font-mono text-gray-300 truncate">
                        {dir}
                      </span>
                      {isDownload && (
                        <span className="badge-purple text-[10px] shrink-0">
                          <Star size={9} className="mr-0.5" /> Download
                        </span>
                      )}
                      {!isDownload && (
                        <button
                          className="btn-ghost text-xs py-0.5 px-1.5"
                          onClick={() => changeDownloadDir(dir)}
                          title="Set as download directory"
                        >
                          <Star size={11} />
                        </button>
                      )}
                      {modelDirs.length > 1 && (
                        <button
                          className="text-gray-600 hover:text-accent-red"
                          onClick={() => removeModelDir(dir)}
                          title="Remove directory"
                        >
                          <Trash2 size={13} />
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Preferred quant sources */}
            <div className="card">
              <h2 className="section-title">Preferred Quant Sources</h2>
              <p className="text-xs text-gray-500 mb-3">
                HuggingFace users/orgs shown in the Browse tab owner filter, in your preferred order.
              </p>
              <PreferredOwners
                owners={preferredOwners}
                onChange={handlePreferredOwnersChange}
              />
            </div>
          </div>
        )}
      </div>

      {/* mmproj picker modal */}
      {mmProjPicker && (
        <div
          className="fixed inset-0 bg-black/60 flex items-center justify-center z-50"
          onClick={() => setMmProjPicker(null)}
        >
          <div
            className="card max-w-md w-full mx-4 space-y-3"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-sm font-semibold text-gray-200">
              Download options
            </h3>
            <p className="text-xs text-gray-400">
              This repo contains vision projection (mmproj) files. Download them alongside the model for vision support.
            </p>
            <div className="space-y-1.5">
              <button
                className="w-full text-left px-3 py-2 rounded hover:bg-surface-3 text-sm text-gray-200"
                onClick={() => handleMmProjChoice(null)}
              >
                Just the model
                <span className="text-xs text-gray-500 ml-2">
                  {formatSize(mmProjPicker.file.size_bytes)}
                </span>
              </button>
              {mmProjPicker.mmProjFiles.map((mp) => (
                <button
                  key={mp.filename}
                  className="w-full text-left px-3 py-2 rounded hover:bg-surface-3 text-sm text-gray-200"
                  onClick={() => handleMmProjChoice(mp)}
                >
                  Model + {mp.filename}
                  <span className="text-xs text-gray-500 ml-2">
                    {formatSize(mmProjPicker.file.size_bytes + mp.size_bytes)}
                  </span>
                </button>
              ))}
            </div>
            <button
              className="btn-ghost text-xs w-full"
              onClick={() => setMmProjPicker(null)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
