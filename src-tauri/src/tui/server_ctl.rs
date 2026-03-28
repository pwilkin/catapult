use std::path::{Path, PathBuf};

use catapult_lib::config::AppConfig;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DetectedServer {
    pub pid: u32,
    pub binary_path: PathBuf,
    pub port: u16,
    pub model_path: Option<String>,
    pub runtime_label: Option<String>,
    pub origin: ServerOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerOrigin {
    Tui,
    External,
    ExternalUnknown,
}

fn pid_file_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("catapult").join("server.pid"))
}

pub fn log_file_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("catapult").join("server.log"))
}

pub fn detect_server(config: &AppConfig) -> Option<DetectedServer> {
    // First check our PID file
    if let Some(tui_server) = check_pid_file(config) {
        return Some(tui_server);
    }

    // Then scan /proc for any llama-server processes
    scan_proc_for_servers(config)
}

fn check_pid_file(config: &AppConfig) -> Option<DetectedServer> {
    let pid_path = pid_file_path()?;
    let content = std::fs::read_to_string(&pid_path).ok()?;
    let mut lines = content.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let binary_path_str = lines.next().unwrap_or("");

    if !process_alive(pid) {
        // Stale PID file — clean up
        let _ = std::fs::remove_file(&pid_path);
        return None;
    }

    let binary_path = if binary_path_str.is_empty() {
        // Try to read from /proc
        std::fs::read_link(format!("/proc/{}/exe", pid)).unwrap_or_default()
    } else {
        PathBuf::from(binary_path_str)
    };

    let port = read_port_from_proc(pid).unwrap_or(8080);
    let model_path = read_arg_from_proc(pid, "--model");
    let runtime_label = match_runtime(&binary_path, config);

    Some(DetectedServer {
        pid,
        binary_path,
        port,
        model_path,
        runtime_label,
        origin: ServerOrigin::Tui,
    })
}

fn scan_proc_for_servers(config: &AppConfig) -> Option<DetectedServer> {
    let proc_dir = std::fs::read_dir("/proc").ok()?;

    for entry in proc_dir.flatten() {
        let pid_str = entry.file_name();
        let pid_str = pid_str.to_str()?;
        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Read cmdline
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        let cmdline = match std::fs::read(&cmdline_path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        // cmdline is null-separated
        let args: Vec<&str> = cmdline
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .filter_map(|s| std::str::from_utf8(s).ok())
            .collect();

        if args.is_empty() {
            continue;
        }

        // Check if this is a llama-server process
        let exe_name = Path::new(args[0])
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if exe_name != "llama-server" && exe_name != "llama-server.exe" {
            continue;
        }

        let binary_path = std::fs::read_link(format!("/proc/{}/exe", pid))
            .unwrap_or_else(|_| PathBuf::from(args[0]));

        let port = args
            .windows(2)
            .find(|w| w[0] == "--port" || w[0] == "-p")
            .and_then(|w| w[1].parse().ok())
            .unwrap_or(8080);

        let model_path = args
            .windows(2)
            .find(|w| w[0] == "--model" || w[0] == "-m")
            .map(|w| w[1].to_string());

        let runtime_label = match_runtime(&binary_path, config);
        let origin = if runtime_label.is_some() {
            ServerOrigin::External
        } else {
            ServerOrigin::ExternalUnknown
        };

        return Some(DetectedServer {
            pid,
            binary_path,
            port,
            model_path,
            runtime_label,
            origin,
        });
    }

    None
}

fn match_runtime(binary_path: &Path, config: &AppConfig) -> Option<String> {
    let binary_str = binary_path.to_string_lossy();

    // Check managed runtimes
    for rt in &config.managed_runtimes {
        if binary_str.contains(&rt.dir_name) {
            return Some(format!("b{} {}", rt.build, rt.backend_label));
        }
    }

    // Check custom runtimes
    for rt in &config.custom_runtimes {
        if binary_path == rt.binary_path {
            return Some(rt.label.clone());
        }
    }

    None
}

fn read_port_from_proc(pid: u32) -> Option<u16> {
    read_arg_from_proc(pid, "--port")
        .or_else(|| read_arg_from_proc(pid, "-p"))
        .and_then(|s| s.parse().ok())
}

fn read_arg_from_proc(pid: u32, flag: &str) -> Option<String> {
    let cmdline = std::fs::read(format!("/proc/{}/cmdline", pid)).ok()?;
    let args: Vec<&str> = cmdline
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|s| std::str::from_utf8(s).ok())
        .collect();
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].to_string())
}

fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

pub fn start_server(
    server_binary: &Path,
    config: &catapult_lib::server::ServerConfig,
    _app_config: &AppConfig,
) -> anyhow::Result<u32> {
    use std::process::Command;

    let args = catapult_lib::server::build_args(config);
    let log_path = log_file_path().ok_or_else(|| anyhow::anyhow!("Cannot determine data directory"))?;

    // Ensure parent dir exists
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write the full command as first line of log
    {
        use std::io::Write;
        let mut header_file = std::fs::File::create(&log_path)?;
        writeln!(
            header_file,
            "# {} {}",
            server_binary.display(),
            args.join(" ")
        )?;
    }

    // Open in append mode for stdout/stderr redirect
    let log_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_path)?;
    let log_err = log_file.try_clone()?;

    let child = Command::new(server_binary)
        .args(&args)
        .stdout(log_file)
        .stderr(log_err)
        .spawn()?;

    let pid = child.id();

    // Write PID file
    let pid_path = pid_file_path().ok_or_else(|| anyhow::anyhow!("Cannot determine data directory"))?;
    std::fs::write(&pid_path, format!("{}\n{}", pid, server_binary.display()))?;

    // Detach — let the process outlive us
    std::mem::forget(child);

    Ok(pid)
}

pub fn stop_server(pid: u32) -> anyhow::Result<()> {
    // Send SIGTERM
    let ret = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if ret != 0 {
        anyhow::bail!("Failed to send SIGTERM to PID {}", pid);
    }

    // Wait up to 30 seconds
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !process_alive(pid) {
            cleanup_pid_file();
            return Ok(());
        }
    }

    // Force kill
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    cleanup_pid_file();
    Ok(())
}

fn cleanup_pid_file() {
    if let Some(path) = pid_file_path() {
        let _ = std::fs::remove_file(path);
    }
}
