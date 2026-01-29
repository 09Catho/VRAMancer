use sysinfo::System;
use std::process::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub ram_total_bytes: u64,
    pub ram_available_bytes: u64,
    pub gpu: GpuInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_total_bytes: u64,
    pub backend: GpuBackend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GpuBackend {
    Nvidia,
    Amd,
    Apple,
    CpuOnly,
    Generic,
}

pub fn detect() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_name = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or("Unknown CPU".to_string());
    let cpu_cores = sys.cpus().len();
    let ram_total_bytes = sys.total_memory();
    let ram_available_bytes = sys.available_memory();

    let gpu = detect_gpu(&mut sys);

    SystemInfo {
        cpu_name,
        cpu_cores,
        ram_total_bytes,
        ram_available_bytes,
        gpu,
    }
}

fn detect_gpu(sys: &mut System) -> GpuInfo {
    // 1. Try NVIDIA
    if let Some(info) = detect_nvidia() {
        return info;
    }

    // 2. Try Apple Silicon
    if let Some(info) = detect_apple(sys) {
        return info;
    }

    // 3. Try AMD (rocm-smi) - simplistic check
    if let Some(info) = detect_amd() {
        return info;
    }

    // 4. Fallback
    GpuInfo {
        name: "CPU (No dedicated GPU detected)".to_string(),
        vram_total_bytes: 0,
        backend: GpuBackend::CpuOnly,
    }
}

fn detect_nvidia() -> Option<GpuInfo> {
    // nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits
    let output = Command::new("nvidia-smi")
        .args(&["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 2 {
        return None;
    }

    let name = parts[0].trim().to_string();
    let memory_mb: u64 = parts[1].trim().parse().ok()?;

    Some(GpuInfo {
        name,
        vram_total_bytes: memory_mb * 1024 * 1024,
        backend: GpuBackend::Nvidia,
    })
}

#[cfg(target_os = "macos")]
fn detect_apple(sys: &mut System) -> Option<GpuInfo> {
    // On Apple Silicon, RAM is VRAM (mostly).
    sys.refresh_cpu();
    let cpu_brand = sys.cpus().first().map(|c| c.brand()).unwrap_or("");

    if cpu_brand.contains("Apple") {
        let total_ram = sys.total_memory();
        return Some(GpuInfo {
            name: format!("{} (Unified Memory)", cpu_brand),
            vram_total_bytes: total_ram,
            backend: GpuBackend::Apple,
        });
    }

    None
}

#[cfg(not(target_os = "macos"))]
fn detect_apple(_sys: &mut System) -> Option<GpuInfo> {
    None
}

fn detect_amd() -> Option<GpuInfo> {
    // rocm-smi --showproductname --showmeminfo vram --json
    let output = Command::new("rocm-smi")
        .args(&["--showproductname", "--showmeminfo", "vram", "--json"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).ok()?;

    if let Some(obj) = json.as_object() {
        for (_key, val) in obj {
             let name = val.get("Card Series").and_then(|v| v.as_str()).unwrap_or("AMD GPU").to_string();
             let vram_str = val.get("VRAM Total Memory (B)").and_then(|v| v.as_str()).unwrap_or("0");
             let vram: u64 = vram_str.parse().unwrap_or(0);

             if vram > 0 {
                 return Some(GpuInfo {
                     name,
                     vram_total_bytes: vram,
                     backend: GpuBackend::Amd,
                  });
             }
        }
    }

    None
}
