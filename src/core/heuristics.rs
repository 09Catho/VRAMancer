use crate::core::system::{GpuBackend, SystemInfo};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Model {
    pub name: String,
    pub family: String,
    pub size_label: String,
    pub total_params_billions: f64,
    pub active_params_billions: f64,
    pub quant: String,
    pub source: ModelSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelSource {
    Ollama,
    HuggingFace,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Estimation {
    pub vram_usage_bytes: u64,
    pub ram_usage_bytes: u64, // if offloaded
    pub vram_status: FitStatus,
    pub tokens_per_sec: f64,
    pub ttft_ms: f64,
    pub recommendation: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FitStatus {
    Fits,
    Partial(String), // Fits with CPU offload
    No,
}

pub fn parse_model_string(name: &str, source: ModelSource) -> Model {
    let name_lower = name.to_lowercase();
    let family = detect_family(&name_lower);
    let (size_label, total_params, active_params) = detect_size(&name_lower, &family);
    let quant = detect_quant(&name_lower);

    Model {
        name: name.to_string(),
        family,
        size_label,
        total_params_billions: total_params,
        active_params_billions: active_params,
        quant,
        source,
    }
}

pub fn estimate_usage(
    model: &Model,
    sys: &SystemInfo,
    ctx_len: usize,
    batch_size: usize,
) -> Estimation {
    let mut notes = Vec::new();

    // 1. Calculate Model Size
    let bpp = get_bits_per_param(&model.quant);
    let weights_size = (model.total_params_billions * 1_000_000_000.0 * (bpp / 8.0)) as u64;

    // 2. Calculate KV Cache
    let (layers, hidden) = guess_arch_params(model);
    // KV Cache (f16): 2 * 2 * layers * hidden * ctx * batch (2 for K and V, 2 for bytes)
    // Actually, usually GQA (Grouped Query Attention) reduces this.
    // Llama 3 has GQA. Mistral has GQA.
    // Let's assume GQA factor of 0.25 (8kv heads vs 32 q heads) for modern models.
    let kv_factor = if model.family.contains("llama")
        || model.family.contains("mistral")
        || model.family.contains("qwen")
    {
        0.25 // aggressive GQA assumption
    } else {
        1.0 // conservative
    };

    let kv_cache_bytes = (2.0
        * 2.0
        * layers as f64
        * hidden as f64
        * ctx_len as f64
        * batch_size as f64
        * kv_factor) as u64;

    // 3. Activation Overhead (rough heuristic: 2% of weights + context dep)
    let overhead_bytes =
        (weights_size as f64 * 0.05) as u64 + (ctx_len * batch_size * hidden * 4) as u64; // rough

    let total_required_memory = weights_size + kv_cache_bytes + overhead_bytes;

    // 4. Fit Check
    let available_vram = sys.gpu.vram_total_bytes;
    let available_ram = sys.ram_available_bytes; // Use available, not total, for safety

    let (vram_usage, ram_usage, status) = if sys.gpu.backend == GpuBackend::CpuOnly {
        (
            0,
            total_required_memory,
            FitStatus::Partial("CPU Only".to_string()),
        )
    } else if sys.gpu.backend == GpuBackend::Apple {
        // Unified memory
        if total_required_memory < available_vram {
            (total_required_memory, 0, FitStatus::Fits)
        } else {
            (
                available_vram,
                total_required_memory - available_vram,
                FitStatus::No,
            ) // Swap?
        }
    } else {
        // Dedicated GPU
        if total_required_memory < available_vram {
            (total_required_memory, 0, FitStatus::Fits)
        } else {
            // Offload
            let spill = total_required_memory.saturating_sub(available_vram);
            if spill < available_ram {
                // Calculate how many layers fit
                let pct_gpu = available_vram as f64 / total_required_memory as f64;
                (
                    available_vram,
                    spill,
                    FitStatus::Partial(format!("Offload {:.0}%", (1.0 - pct_gpu) * 100.0)),
                )
            } else {
                (available_vram, available_ram, FitStatus::No)
            }
        }
    };

    if status == FitStatus::No && sys.gpu.backend == GpuBackend::Apple {
        // Apple swap is fast, maybe "Partial" instead of No?
        // But if it exceeds RAM, it's definitely NO (thrashing).
        // Let's stick to NO if > total RAM.
    }

    // 5. Performance Estimation
    // Bandwidth assumption
    let memory_bandwidth_gbps = match sys.gpu.backend {
        GpuBackend::Nvidia => 500.0, // Mid-range assumption (3060/4060 is ~300, 3090 is ~900).
        // Ideally we'd look up by GPU name, but that's complex.
        GpuBackend::Amd => 400.0,
        GpuBackend::Apple => 100.0, // M1 base is 60, M1 Max is 400. Let's vary by name if possible or conservatively 100.
        GpuBackend::CpuOnly => 40.0, // DDR4/5 dual channel
        GpuBackend::Generic => 50.0,
    };

    // Tok/s = Bandwidth / Active_Bytes
    // If offloading, bandwidth is weighted average of GPU and RAM BW (RAM is slow).
    let active_params_size = (model.active_params_billions * 1_000_000_000.0 * (bpp / 8.0)) as u64;
    // We read all active weights per token.
    // KV cache is read too (context dependent), but let's focus on weights for simple calc.

    let effective_bw = if ram_usage > 0 {
        // Penalty for offload. PCIe bottleneck ~16GB/s or DDR speed.
        // Harmonic mean or weighted?
        // If 50% layers on GPU, 50% on CPU. Speed is limited by slowest stage usually if pipelined,
        // or sum of latencies.
        // Time = (Bytes_GPU / BW_GPU) + (Bytes_CPU / BW_CPU).
        // BW_CPU here is system RAM BW ~40GB/s.
        let bytes_gpu =
            active_params_size as f64 * (vram_usage as f64 / total_required_memory as f64);
        let bytes_cpu = active_params_size as f64 - bytes_gpu;

        let t_gpu = bytes_gpu / (memory_bandwidth_gbps * 1e9);
        let t_cpu = bytes_cpu / (30.0 * 1e9); // 30 GB/s RAM assumption

        active_params_size as f64 / (t_gpu + t_cpu) / 1e9 // Effective GB/s
    } else {
        memory_bandwidth_gbps
    };

    let tokens_per_sec = (effective_bw * 1e9) / active_params_size as f64;

    // TTFT
    // Prefill depends on compute (FLOPS) mostly.
    // Rough guess: 1ms per token in batch?
    // Very rough: tokens_per_sec * 0.5 for prefill speed? No, prefill is faster (compute bound vs memory bound).
    // Let's assume prefill is 10x decoding speed for small batches, or BW limited for large prompts.
    // TTFT = ctx_len / (tokens_per_sec * 5.0); // complete guess
    let ttft_ms = (ctx_len as f64 / (tokens_per_sec * 10.0)) * 1000.0;

    // Recommendations
    let recommendation = if vram_usage < sys.gpu.vram_total_bytes {
        "Excellent. Run entirely on GPU.".to_string()
    } else if ram_usage > 0 && matches!(status, FitStatus::Partial(_)) {
        "Functional but slower due to CPU offloading.".to_string()
    } else {
        "Model is too large for this system.".to_string()
    };

    notes.push(format!(
        "Assumed {:.1} bits per weight ({})",
        bpp, model.quant
    ));
    if model.family == "mixtral" {
        notes.push("Mixtral MoE: Active params used for speed estimate.".to_string());
    }

    Estimation {
        vram_usage_bytes: vram_usage,
        ram_usage_bytes: ram_usage,
        vram_status: status,
        tokens_per_sec,
        ttft_ms,
        recommendation,
        notes,
    }
}

fn get_bits_per_param(quant: &str) -> f64 {
    match quant {
        q if q.contains("q8") => 8.5, // + overhead
        q if q.contains("q6") => 6.5,
        q if q.contains("q5") => 5.5,
        q if q.contains("q4") => 5.0, // 4.5 - 5.0 safe bet
        q if q.contains("q3") => 3.5,
        q if q.contains("q2") => 2.5, // 2.56
        q if q.contains("fp16") || q.contains("f16") => 16.0,
        q if q.contains("bf16") => 16.0,
        q if q.contains("fp32") => 32.0,
        _ => 16.0, // assume float16
    }
}

fn guess_arch_params(model: &Model) -> (usize, usize) {
    // Family based defaults
    // Returns (layers, hidden_size)
    match model.family.as_str() {
        "llama" => {
            if model.total_params_billions > 60.0 {
                (80, 8192)
            }
            // 70b
            else if model.total_params_billions > 10.0 {
                (40, 5120)
            }
            // 13b/default
            else {
                (32, 4096)
            } // 8b
        }
        "mistral" => (32, 4096),
        "mixtral" => (32, 4096),
        "qwen" => {
            if model.total_params_billions > 10.0 {
                (40, 5120)
            }
            // 14b
            else {
                (32, 4096)
            } // 7b/smaller
        }
        "gemma" => (18, 2048), // 2b is smaller
        "phi" => (32, 2560),   // phi-2/3 roughly
        _ => (32, 4096),       // Generic
    }
}

fn detect_family(name: &str) -> String {
    // Basic heuristics
    if name.contains("llama") {
        return "llama".to_string();
    }
    if name.contains("mistral") {
        return "mistral".to_string();
    }
    if name.contains("mixtral") {
        return "mixtral".to_string();
    }
    if name.contains("qwen") {
        return "qwen".to_string();
    }
    if name.contains("gemma") {
        return "gemma".to_string();
    }
    if name.contains("phi") {
        return "phi".to_string();
    }
    if name.contains("yi") {
        return "yi".to_string();
    }
    if name.contains("falcon") {
        return "falcon".to_string();
    }
    if name.contains("starcoder") {
        return "starcoder".to_string();
    }
    if name.contains("deepseek") {
        return "deepseek".to_string();
    }
    if name.contains("command") {
        return "command-r".to_string();
    }

    "unknown".to_string()
}

fn detect_size(name: &str, family: &str) -> (String, f64, f64) {
    // Regex for "NxM b" or "Nb"
    static MOE_REGEX: OnceLock<Regex> = OnceLock::new();
    let moe_re = MOE_REGEX.get_or_init(|| Regex::new(r"(\d+)x(\d+(\.\d+)?)b").unwrap());

    static SIZE_REGEX: OnceLock<Regex> = OnceLock::new();
    let size_re = SIZE_REGEX.get_or_init(|| Regex::new(r"(\d+(\.\d+)?)b").unwrap());

    if let Some(caps) = moe_re.captures(name) {
        let count: f64 = caps[1].parse().unwrap_or(1.0);
        let per_expert: f64 = caps[2].parse().unwrap_or(0.0);
        let total = count * per_expert;
        let active = if family == "mixtral" {
            12.9
        } else {
            per_expert * 2.0
        };
        return (caps[0].to_string(), total, active);
    }

    if let Some(caps) = size_re.captures(name) {
        let size: f64 = caps[1].parse().unwrap_or(0.0);
        return (caps[0].to_string(), size, size);
    }

    // Fallbacks
    match family {
        "llama" => ("7b".to_string(), 7.0, 7.0),
        "mistral" => ("7b".to_string(), 7.0, 7.0),
        "mixtral" => ("8x7b".to_string(), 47.0, 12.9),
        "gemma" => ("2b".to_string(), 2.0, 2.0),
        "qwen" => ("7b".to_string(), 7.0, 7.0),
        "phi" => ("3b".to_string(), 3.0, 3.0),
        // Add deepseek defaults
        "deepseek" => ("67b".to_string(), 67.0, 67.0), // Guess high or 67b default
        _ => ("unknown".to_string(), 7.0, 7.0),
    }
}

fn detect_quant(name: &str) -> String {
    static QUANT_REGEX: OnceLock<Regex> = OnceLock::new();
    let quant_re =
        QUANT_REGEX.get_or_init(|| Regex::new(r"(q[234568](_?[0-9A-Z]+)?|fp16|bf16)").unwrap());

    if let Some(caps) = quant_re.captures(name) {
        return caps[0].to_string();
    }

    "f16".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llama_parse() {
        let m = parse_model_string("llama3:8b", ModelSource::Ollama);
        assert_eq!(m.family, "llama");
        assert_eq!(m.total_params_billions, 8.0);
        assert_eq!(m.quant, "f16");
    }

    #[test]
    fn test_estimation() {
        let m = parse_model_string("llama3:8b-q4_0", ModelSource::Ollama);
        let sys = SystemInfo {
            cpu_name: "TestCPU".to_string(),
            cpu_cores: 8,
            ram_total_bytes: 32_000_000_000,
            ram_available_bytes: 16_000_000_000,
            gpu: crate::core::system::GpuInfo {
                name: "TestGPU".to_string(),
                vram_total_bytes: 12_000_000_000, // 12GB
                backend: GpuBackend::Nvidia,
            },
        };

        // 8B params * 0.625 bytes (5 bits) = 5GB approx.
        // KV cache for 4k ctx = 0.5GB approx.
        // Total ~5.5GB. Should fit in 12GB.

        let est = estimate_usage(&m, &sys, 4096, 1);
        assert_eq!(est.vram_status, FitStatus::Fits);
        assert!(est.vram_usage_bytes > 4_000_000_000);
        assert!(est.vram_usage_bytes < 8_000_000_000);
    }
}
