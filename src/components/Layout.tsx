import { NavLink, Outlet, useNavigate, useLocation } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useEffect, useState } from "react";
import {
  LayoutDashboard,
  Download,
  Database,
  Play,
  MessageSquare,
  Minus,
  Square,
  Layers,
  X,
  ArrowUpCircle,
  RefreshCw,
} from "lucide-react";
import { clsx } from "clsx";
import CatapultIcon from "./CatapultIcon";

const navItems = [
  { to: "/dashboard", label: "Dashboard", icon: LayoutDashboard },
  { to: "/runtime", label: "Runtime", icon: Download },
  { to: "/models", label: "Models", icon: Database },
  { to: "/server", label: "Run", icon: Play },
  { to: "/chat", label: "Chat", icon: MessageSquare },
];

type Update = Awaited<ReturnType<typeof check>>;

function VersionInfo() {
  const [version, setVersion] = useState<string | null>(null);
  const [update, setUpdate] = useState<Update>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    getVersion().then(setVersion);
    check().then(setUpdate).catch(() => {});
  }, []);

  const handleUpdate = async () => {
    if (!update || installing) return;
    setInstalling(true);
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch {
      setInstalling(false);
    }
  };

  if (!version) return null;

  return (
    <div className="relative z-10 flex items-center gap-1.5 ml-2">
      <span className="text-xs text-gray-500 select-none tabular-nums">v{version}</span>
      {update && (
        <button
          onClick={handleUpdate}
          disabled={installing}
          className="flex items-center text-primary-light hover:text-primary transition-colors disabled:opacity-50"
          title={installing ? "Installing update…" : `v${update.version} available — click to install & restart`}
        >
          {installing
            ? <RefreshCw size={13} className="animate-spin" />
            : <ArrowUpCircle size={14} />}
        </button>
      )}
    </div>
  );
}

function WindowControls() {
  const appWindow = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    appWindow.isMaximized().then(setMaximized);
    let debounce: ReturnType<typeof setTimeout>;
    const unlisten = appWindow.onResized(() => {
      clearTimeout(debounce);
      debounce = setTimeout(() => {
        appWindow.isMaximized().then(setMaximized);
      }, 100);
    });
    return () => {
      clearTimeout(debounce);
      unlisten.then((f) => f());
    };
  }, []);

  return (
    <div className="flex items-center">
      <button
        onClick={() => appWindow.minimize()}
        className="w-10 h-11 flex items-center justify-center text-gray-400 hover:text-gray-200 hover:bg-white/5 transition-colors"
        title="Minimize"
      >
        <Minus size={14} />
      </button>
      <button
        onClick={() => appWindow.toggleMaximize()}
        className="w-10 h-11 flex items-center justify-center text-gray-400 hover:text-gray-200 hover:bg-white/5 transition-colors"
        title={maximized ? "Restore" : "Maximize"}
      >
        {maximized ? <Layers size={12} /> : <Square size={11} />}
      </button>
      <button
        onClick={() => appWindow.close()}
        className="w-10 h-11 flex items-center justify-center text-gray-400 hover:text-white hover:bg-red-600 transition-colors"
        title="Close"
      >
        <X size={14} />
      </button>
    </div>
  );
}

export default function Layout() {
  const navigate = useNavigate();
  const location = useLocation();
  const onDashboard = location.pathname === "/dashboard";

  return (
    <div className="flex flex-col h-full bg-surface-0">
      {/* Title bar — custom, replaces OS decorations */}
      <div
        className="relative flex items-center h-11 px-3 border-b border-primary/25 shrink-0 bg-primary/8"
      >
        {/* Drag region — fills entire title bar behind interactive elements */}
        <div
          className="absolute inset-0"
          onMouseDown={(e) => {
            if (e.button === 0) getCurrentWindow().startDragging();
          }}
          onDoubleClick={() => getCurrentWindow().toggleMaximize()}
        />

        {/* Logo / home */}
        <button
          onClick={() => navigate("/dashboard")}
          disabled={onDashboard}
          className={`relative z-10 flex items-center gap-2 px-1.5 py-1 -ml-1 rounded transition-colors ${
            onDashboard
              ? "cursor-default"
              : "hover:bg-primary/15 active:bg-primary/25"
          }`}
          title={onDashboard ? "Catapult" : "Back to Dashboard"}
        >
          <CatapultIcon size={22} className="text-primary-light" />
          <span className="text-sm font-semibold text-gray-200 tracking-tight select-none">
            Catapult
          </span>
        </button>

        {/* Version + update indicator */}
        <VersionInfo />

        {/* Separator */}
        <div className="relative z-10 w-px h-5 bg-primary/20 mx-3" />

        {/* Nav */}
        <nav className="relative z-10 flex items-center gap-0.5">
          {navItems.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                clsx(
                  "flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded transition-colors",
                  isActive
                    ? "bg-primary/20 text-primary-light"
                    : "text-gray-400 hover:text-gray-200 hover:bg-primary/10"
                )
              }
            >
              <Icon size={13} />
              {label}
            </NavLink>
          ))}
        </nav>

        {/* Window controls */}
        <div className="relative z-10 ml-auto">
          <WindowControls />
        </div>
      </div>

      {/* Main */}
      <main className="flex-1 overflow-hidden flex flex-col min-w-0">
        <Outlet />
      </main>
    </div>
  );
}
