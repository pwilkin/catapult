use anyhow::Result;
use serde::{Deserialize, Serialize};
use sysinfo::System;

/// Creates a Command that won't spawn a visible console window on Windows.
fn silent_cmd(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_name: String,
    pub cpu_cores: u32,
    pub cpu_threads: u32,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub gpus: Vec<GpuInfo>,
    pub os: String,
    pub arch: String,
    pub available_backends: Vec<BackendInfo>,
    pub recommended_backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
    pub vendor: GpuVendor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedConfig {
    pub n_gpu_layers: i32,
    pub n_ctx: u32,
    pub can_fit_fully_in_vram: bool,
    pub total_usable_mb: u64,
    pub notes: Vec<String>,
}

pub fn get_system_info() -> Result<SystemInfo> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let cpu_cores = sys.physical_core_count().unwrap_or(1) as u32;
    let cpu_threads = sys.cpus().len() as u32;
    let total_ram_mb = sys.total_memory() / (1024 * 1024);
    let available_ram_mb = sys.available_memory() / (1024 * 1024);

    let gpus = detect_gpus();
    let available_backends = detect_backends(&gpus);
    let recommended_backend = pick_best_backend(&available_backends, &gpus);

    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    Ok(SystemInfo {
        cpu_name,
        cpu_cores,
        cpu_threads,
        total_ram_mb,
        available_ram_mb,
        gpus,
        os,
        arch,
        available_backends,
        recommended_backend,
    })
}

fn detect_gpus() -> Vec<GpuInfo> {
    #[cfg(target_os = "linux")]
    return detect_gpus_linux();
    #[cfg(target_os = "windows")]
    return detect_gpus_windows();
    #[cfg(target_os = "macos")]
    return detect_gpus_macos();
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    return vec![];
}

#[cfg(target_os = "linux")]
fn detect_gpus_linux() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();

    // Try nvidia-smi first for NVIDIA GPUs
    if let Ok(output) = silent_cmd("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.splitn(2, ',').collect();
                if parts.len() == 2 {
                    let name = parts[0].trim().to_string();
                    let vram_mb = parts[1].trim().parse::<u64>().unwrap_or(0);
                    gpus.push(GpuInfo {
                        name,
                        vram_mb,
                        vendor: GpuVendor::Nvidia,
                    });
                }
            }
        }
    }

    // Try rocm-smi for AMD GPUs
    if let Ok(output) = silent_cmd("rocm-smi")
        .args(["--showmeminfo", "vram", "--json"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(cards) = json.as_object() {
                    for (_, card) in cards {
                        let name = card["Card series"].as_str().unwrap_or("AMD GPU").to_string();
                        let vram_str = card["VRAM Total Memory (B)"].as_str().unwrap_or("0");
                        let vram_mb = vram_str.parse::<u64>().unwrap_or(0) / (1024 * 1024);
                        if !gpus.iter().any(|g| g.vendor == GpuVendor::Amd) {
                            gpus.push(GpuInfo {
                                name,
                                vram_mb,
                                vendor: GpuVendor::Amd,
                            });
                        }
                    }
                }
            }
        }
    }

    // Fallback: parse lspci
    if gpus.is_empty() {
        if let Ok(output) = silent_cmd("lspci").output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let lower = line.to_lowercase();
                if lower.contains("vga") || lower.contains("3d controller") || lower.contains("display controller") {
                    let vendor = if lower.contains("nvidia") {
                        GpuVendor::Nvidia
                    } else if lower.contains("amd") || lower.contains("radeon") || lower.contains("advanced micro") {
                        GpuVendor::Amd
                    } else if lower.contains("intel") {
                        GpuVendor::Intel
                    } else {
                        GpuVendor::Unknown
                    };

                    // Extract GPU name (part after the colon)
                    let name = line
                        .split(':')
                        .last()
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|| "Unknown GPU".to_string());

                    gpus.push(GpuInfo {
                        name,
                        vram_mb: 0,
                        vendor,
                    });
                }
            }
        }
    }

    gpus
}

/// Returns true for virtual/emulated GPU adapters that should be deprioritized.
#[cfg(any(target_os = "windows", test))]
fn is_virtual_gpu(name: &str) -> bool {
    let lower = name.to_lowercase();
    let virtual_keywords = [
        "microsoft basic display",
        "microsoft hyper-v video",
        "microsoft remote display",
        "vmware svga",
        "virtualbox",
        "parallels display",
        "qemu",
        "red hat qxl",
        "aspeed",
        "virtual render",
    ];
    virtual_keywords.iter().any(|kw| lower.contains(kw))
}

#[cfg(target_os = "windows")]
fn detect_gpus_windows() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();

    // Use PowerShell to query WMI
    let script = "Get-WmiObject Win32_VideoController | Select-Object Name,AdapterRAM | ConvertTo-Json";
    if let Ok(output) = silent_cmd("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            // Handle both single object and array
            let json_text = if text.trim().starts_with('[') {
                text.to_string()
            } else {
                format!("[{}]", text)
            };

            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&json_text) {
                for item in arr {
                    let name = item["Name"].as_str().unwrap_or("Unknown GPU").to_string();
                    let vram_mb = item["AdapterRAM"]
                        .as_u64()
                        .unwrap_or(0)
                        / (1024 * 1024);
                    let lower = name.to_lowercase();
                    let vendor = if lower.contains("nvidia") || lower.contains("geforce") {
                        GpuVendor::Nvidia
                    } else if lower.contains("amd") || lower.contains("radeon") {
                        GpuVendor::Amd
                    } else if lower.contains("intel") {
                        GpuVendor::Intel
                    } else {
                        GpuVendor::Unknown
                    };

                    // Try nvidia-smi for more accurate VRAM
                    let actual_vram = if vendor == GpuVendor::Nvidia {
                        get_nvidia_vram_mb().unwrap_or(vram_mb)
                    } else {
                        vram_mb
                    };

                    gpus.push(GpuInfo {
                        name,
                        vram_mb: actual_vram,
                        vendor,
                    });
                }
            }
        }
    }

    // Filter out virtual GPUs when real ones are present
    let has_real_gpu = gpus.iter().any(|g| !is_virtual_gpu(&g.name));
    if has_real_gpu {
        gpus.retain(|g| !is_virtual_gpu(&g.name));
    }

    gpus
}

#[cfg(target_os = "macos")]
fn detect_gpus_macos() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();

    if let Ok(output) = silent_cmd("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(displays) = json["SPDisplaysDataType"].as_array() {
                    for display in displays {
                        let name = display["sppci_model"]
                            .as_str()
                            .unwrap_or("Apple GPU")
                            .to_string();

                        // VRAM parsing (e.g., "16 GB")
                        let vram_mb = display["spdisplays_vram"]
                            .as_str()
                            .and_then(|s| parse_vram_string(s))
                            .unwrap_or(0);

                        let vendor = if name.to_lowercase().contains("apple") {
                            GpuVendor::Apple
                        } else if name.to_lowercase().contains("amd") {
                            GpuVendor::Amd
                        } else {
                            GpuVendor::Intel
                        };

                        gpus.push(GpuInfo { name, vram_mb, vendor });
                    }
                }
            }
        }
    }

    if gpus.is_empty() {
        gpus.push(GpuInfo {
            name: "Apple Silicon GPU".to_string(),
            vram_mb: 0, // shared memory, unknown
            vendor: GpuVendor::Apple,
        });
    }

    gpus
}

#[allow(dead_code)]
fn parse_vram_string(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 2 {
        let value: f64 = parts[0].parse().ok()?;
        let multiplier = match parts[1].to_uppercase().as_str() {
            "GB" => 1024u64,
            "MB" => 1u64,
            _ => return None,
        };
        Some((value as u64) * multiplier)
    } else {
        None
    }
}

#[allow(dead_code)]
fn get_nvidia_vram_mb() -> Option<u64> {
    let output = silent_cmd("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        let mb: u64 = text.trim().parse().ok()?;
        Some(mb)
    } else {
        None
    }
}

#[cfg_attr(target_os = "macos", allow(unused_variables))]
fn detect_backends(gpus: &[GpuInfo]) -> Vec<BackendInfo> {
    let mut backends = vec![BackendInfo {
        id: "cpu".to_string(),
        name: "CPU".to_string(),
        available: true,
        description: "Run on CPU (AVX2). Slowest but always available.".to_string(),
    }];

    #[cfg(target_os = "linux")]
    {
        // CUDA (via nvidia-smi presence)
        let cuda_available = gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia)
            && silent_cmd("nvidia-smi").output().map(|o| o.status.success()).unwrap_or(false);
        backends.push(BackendInfo {
            id: "cuda".to_string(),
            name: "CUDA".to_string(),
            available: cuda_available,
            description: "NVIDIA GPU acceleration via CUDA.".to_string(),
        });

        // ROCm (AMD)
        let rocm_available = gpus.iter().any(|g| g.vendor == GpuVendor::Amd)
            && (std::path::Path::new("/opt/rocm").exists()
                || silent_cmd("rocm-smi").output().map(|o| o.status.success()).unwrap_or(false));
        backends.push(BackendInfo {
            id: "rocm".to_string(),
            name: "ROCm (HIP)".to_string(),
            available: rocm_available,
            description: "AMD GPU acceleration via ROCm/HIP.".to_string(),
        });

        // Vulkan
        let vulkan_available = !gpus.is_empty()
            && (std::path::Path::new("/usr/lib/libvulkan.so.1").exists()
                || std::path::Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so.1").exists()
                || silent_cmd("vulkaninfo").arg("--summary").output().map(|o| o.status.success()).unwrap_or(false));
        backends.push(BackendInfo {
            id: "vulkan".to_string(),
            name: "Vulkan".to_string(),
            available: vulkan_available,
            description: "GPU acceleration via Vulkan (AMD/NVIDIA/Intel).".to_string(),
        });

        // OpenVINO (Intel)
        let openvino_available = gpus.iter().any(|g| g.vendor == GpuVendor::Intel)
            && (std::path::Path::new("/opt/intel/openvino").exists()
                || std::path::Path::new("/usr/lib/libopenvino.so").exists());
        backends.push(BackendInfo {
            id: "openvino".to_string(),
            name: "OpenVINO".to_string(),
            available: openvino_available,
            description: "Intel GPU/NPU acceleration via OpenVINO.".to_string(),
        });
    }

    #[cfg(target_os = "windows")]
    {
        // CUDA
        let cuda_available = gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia)
            && silent_cmd("nvidia-smi").output().map(|o| o.status.success()).unwrap_or(false);
        backends.push(BackendInfo {
            id: "cuda".to_string(),
            name: "CUDA".to_string(),
            available: cuda_available,
            description: "NVIDIA GPU acceleration via CUDA.".to_string(),
        });

        // Vulkan
        let vulkan_available = !gpus.is_empty() && {
            let sys32 = std::env::var("SYSTEMROOT").unwrap_or_else(|_| "C:\\Windows".to_string());
            std::path::Path::new(&format!("{}\\System32\\vulkan-1.dll", sys32)).exists()
        };
        backends.push(BackendInfo {
            id: "vulkan".to_string(),
            name: "Vulkan".to_string(),
            available: vulkan_available,
            description: "GPU acceleration via Vulkan (AMD/NVIDIA/Intel).".to_string(),
        });

        // SYCL (Intel oneAPI)
        let sycl_available = gpus.iter().any(|g| g.vendor == GpuVendor::Intel) && {
            let oneapi = std::path::Path::new("C:\\Program Files (x86)\\Intel\\oneAPI").exists()
                || std::path::Path::new("C:\\Program Files\\Intel\\oneAPI").exists();
            oneapi
        };
        backends.push(BackendInfo {
            id: "sycl".to_string(),
            name: "SYCL (Intel oneAPI)".to_string(),
            available: sycl_available,
            description: "Intel GPU acceleration via SYCL/oneAPI.".to_string(),
        });

        // HIP (AMD on Windows)
        let hip_available = gpus.iter().any(|g| g.vendor == GpuVendor::Amd) && {
            std::path::Path::new("C:\\Program Files\\AMD\\ROCm").exists()
        };
        backends.push(BackendInfo {
            id: "hip".to_string(),
            name: "HIP (AMD ROCm)".to_string(),
            available: hip_available,
            description: "AMD GPU acceleration via HIP.".to_string(),
        });
    }

    #[cfg(target_os = "macos")]
    {
        backends.push(BackendInfo {
            id: "metal".to_string(),
            name: "Metal".to_string(),
            available: true,
            description: "Apple GPU acceleration via Metal.".to_string(),
        });
    }

    backends
}

fn pick_best_backend(backends: &[BackendInfo], gpus: &[GpuInfo]) -> String {
    // Priority: CUDA > Metal > ROCm > Vulkan > SYCL > HIP > OpenVINO > CPU
    let priority = ["cuda", "metal", "rocm", "vulkan", "hip", "sycl", "openvino", "cpu"];

    for &id in &priority {
        if let Some(b) = backends.iter().find(|b| b.id == id && b.available) {
            // For Vulkan, prefer only if there's a discrete GPU
            if id == "vulkan" && !gpus.iter().any(|g| g.vram_mb > 512) {
                continue;
            }
            return b.id.clone();
        }
    }

    "cpu".to_string()
}

pub fn suggest_config(model_size_mb: u64, system: &SystemInfo) -> SuggestedConfig {
    let total_vram_mb: u64 = system.gpus.iter().map(|g| g.vram_mb).sum();
    let total_ram_mb = system.available_ram_mb;
    let mut notes = Vec::new();

    let (n_gpu_layers, can_fit_fully_in_vram) = if total_vram_mb > 0 {
        let usable_vram = total_vram_mb.saturating_sub(512); // reserve 512MB for overhead
        if model_size_mb <= usable_vram {
            notes.push("Model fits entirely in VRAM - full GPU acceleration.".to_string());
            (-1i32, true) // -1 = all layers
        } else if model_size_mb <= usable_vram + total_ram_mb {
            // Partial offload: estimate layers
            let ratio = usable_vram as f64 / model_size_mb as f64;
            let estimated_layers = (ratio * 32.0) as i32; // assume ~32 layers typical
            notes.push(format!(
                "Model partially fits in VRAM ({:.0}%). Offloading ~{} layers to GPU.",
                ratio * 100.0,
                estimated_layers
            ));
            (estimated_layers, false)
        } else {
            notes.push("Model too large for GPU+RAM. CPU only.".to_string());
            (0i32, false)
        }
    } else {
        if model_size_mb > total_ram_mb.saturating_sub(1024) {
            notes.push("Warning: Model may not fit in available RAM.".to_string());
        }
        (0i32, false)
    };

    // Context size: 0 means "loaded from model" (llama-server default)
    let n_ctx = 0u32;

    let total_usable_mb = if total_vram_mb > 0 {
        total_vram_mb + total_ram_mb
    } else {
        total_ram_mb
    };

    SuggestedConfig {
        n_gpu_layers,
        n_ctx,
        can_fit_fully_in_vram,
        total_usable_mb,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_system(vram_mb: u64, ram_mb: u64) -> SystemInfo {
        let gpus = if vram_mb > 0 {
            vec![GpuInfo { name: "Test GPU".to_string(), vram_mb, vendor: GpuVendor::Nvidia }]
        } else {
            vec![]
        };
        SystemInfo {
            cpu_name: "Test CPU".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            total_ram_mb: ram_mb,
            available_ram_mb: ram_mb,
            gpus,
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            available_backends: vec![],
            recommended_backend: "cpu".to_string(),
        }
    }

    #[test]
    fn suggest_config_fits_in_vram() {
        let system = make_system(8192, 16384); // 8GB VRAM, 16GB RAM
        let config = suggest_config(4000, &system); // 4GB model
        assert_eq!(config.n_gpu_layers, -1);
        assert!(config.can_fit_fully_in_vram);
    }

    #[test]
    fn suggest_config_partial_offload() {
        let system = make_system(8192, 32768); // 8GB VRAM, 32GB RAM
        let config = suggest_config(12000, &system); // 12GB model
        assert!(config.n_gpu_layers > 0, "should partially offload");
        assert!(config.n_gpu_layers < 32, "should not offload all layers");
        assert!(!config.can_fit_fully_in_vram);
    }

    #[test]
    fn suggest_config_no_gpu() {
        let system = make_system(0, 16384); // no GPU, 16GB RAM
        let config = suggest_config(4000, &system);
        assert_eq!(config.n_gpu_layers, 0);
        assert!(!config.can_fit_fully_in_vram);
    }

    #[test]
    fn suggest_config_model_too_large() {
        let system = make_system(8192, 8192); // 8GB VRAM + 8GB RAM
        let config = suggest_config(50000, &system); // 50GB model
        assert_eq!(config.n_gpu_layers, 0);
        assert!(!config.can_fit_fully_in_vram);
    }

    #[test]
    fn suggest_config_context_is_zero() {
        let system = make_system(8192, 16384);
        let config = suggest_config(4000, &system);
        assert_eq!(config.n_ctx, 0, "should default to 0 (model default)");
    }

    // ── is_virtual_gpu ──────────────────────────────────────────────────────

    #[test]
    fn virtual_gpu_detects_hyper_v() {
        assert!(is_virtual_gpu("Microsoft Hyper-V Video"));
    }

    #[test]
    fn virtual_gpu_detects_basic_display() {
        assert!(is_virtual_gpu("Microsoft Basic Display Adapter"));
    }

    #[test]
    fn virtual_gpu_detects_vmware() {
        assert!(is_virtual_gpu("VMware SVGA 3D"));
    }

    #[test]
    fn virtual_gpu_detects_virtualbox() {
        assert!(is_virtual_gpu("VirtualBox Graphics Adapter (WDDM)"));
    }

    #[test]
    fn virtual_gpu_case_insensitive() {
        assert!(is_virtual_gpu("MICROSOFT BASIC DISPLAY ADAPTER"));
        assert!(is_virtual_gpu("vmware svga"));
    }

    #[test]
    fn virtual_gpu_rejects_real_nvidia() {
        assert!(!is_virtual_gpu("NVIDIA GeForce RTX 4090"));
    }

    #[test]
    fn virtual_gpu_rejects_real_amd() {
        assert!(!is_virtual_gpu("AMD Radeon RX 7900 XTX"));
    }

    #[test]
    fn virtual_gpu_rejects_real_intel() {
        assert!(!is_virtual_gpu("Intel Arc A770"));
    }

    // ── virtual GPU filtering ───────────────────────────────────────────────

    #[test]
    fn filter_virtual_gpus_when_real_present() {
        let mut gpus = vec![
            GpuInfo { name: "Microsoft Hyper-V Video".into(), vram_mb: 128, vendor: GpuVendor::Unknown },
            GpuInfo { name: "NVIDIA GeForce RTX 4090".into(), vram_mb: 24576, vendor: GpuVendor::Nvidia },
        ];
        let has_real = gpus.iter().any(|g| !is_virtual_gpu(&g.name));
        if has_real {
            gpus.retain(|g| !is_virtual_gpu(&g.name));
        }
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 4090");
    }

    #[test]
    fn keep_virtual_gpus_when_no_real_present() {
        let mut gpus = vec![
            GpuInfo { name: "Microsoft Hyper-V Video".into(), vram_mb: 128, vendor: GpuVendor::Unknown },
            GpuInfo { name: "Microsoft Basic Display Adapter".into(), vram_mb: 0, vendor: GpuVendor::Unknown },
        ];
        let has_real = gpus.iter().any(|g| !is_virtual_gpu(&g.name));
        if has_real {
            gpus.retain(|g| !is_virtual_gpu(&g.name));
        }
        assert_eq!(gpus.len(), 2, "should keep all GPUs when only virtual ones exist");
    }

    // ── silent_cmd ──────────────────────────────────────────────────────────

    #[test]
    fn silent_cmd_creates_command() {
        // Verify silent_cmd produces a runnable Command (will fail to find
        // the binary, but must not panic during construction).
        let mut cmd = silent_cmd("nonexistent-binary-12345");
        let result = cmd.output();
        assert!(result.is_err(), "nonexistent binary should fail");
    }
}
