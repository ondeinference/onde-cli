//! LoRA adapter merge module.
//!
//! Merges LoRA adapter weights (produced by the fine-tuner) back into the base
//! model safetensors, producing a single merged `model.safetensors` alongside
//! copied tokenizer and config files.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use candle_core::safetensors::MmapedSafetensors;
use candle_core::{DType, Device, Tensor};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct MergeConfig {
    /// Snapshot directory containing `model.safetensors` (or sharded files) and
    /// `config.json`.
    pub base_dir: PathBuf,
    /// Path to the `lora_adapter.safetensors` produced by the fine-tuner.
    pub adapter_path: PathBuf,
    /// Directory where the merged model and copied config/tokenizer files will
    /// be written.
    pub output_dir: PathBuf,
}

pub enum MergeProgress {
    Loading,
    Merging { layer: usize, total: usize },
    Saving,
    Done { output_path: PathBuf },
    Failed(String),
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn start_merge(config: MergeConfig, tx: tokio::sync::mpsc::UnboundedSender<MergeProgress>) {
    std::thread::spawn(move || {
        if let Err(e) = run_merge(&config, &tx) {
            eprintln!("[merge] error: {e:#}");
            let _ = tx.send(MergeProgress::Failed(format!("{e:#}")));
        }
    });
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct MinimalConfig {
    num_hidden_layers: usize,
}

#[derive(serde::Deserialize)]
struct IndexJson {
    weight_map: HashMap<String, String>,
}

fn run_merge(
    config: &MergeConfig,
    tx: &tokio::sync::mpsc::UnboundedSender<MergeProgress>,
) -> Result<()> {
    let _ = tx.send(MergeProgress::Loading);

    // 1. Read num_hidden_layers from config.json
    let config_path = config.base_dir.join("config.json");
    let config_text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let model_cfg: MinimalConfig =
        serde_json::from_str(&config_text).context("failed to parse config.json")?;
    let num_layers = model_cfg.num_hidden_layers;

    // 2. Load base safetensors (single file or sharded)
    let index_path = config.base_dir.join("model.safetensors.index.json");
    let base_paths: Vec<PathBuf> = if index_path.exists() {
        let index_text = std::fs::read_to_string(&index_path)
            .with_context(|| format!("failed to read {}", index_path.display()))?;
        let index: IndexJson = serde_json::from_str(&index_text)
            .context("failed to parse model.safetensors.index.json")?;
        let mut filenames: Vec<String> = index.weight_map.into_values().collect();
        filenames.sort();
        filenames.dedup();
        filenames
            .into_iter()
            .map(|f| config.base_dir.join(f))
            .collect()
    } else {
        vec![config.base_dir.join("model.safetensors")]
    };

    // SAFETY: the files are memory-mapped read-only.
    let base = if base_paths.len() == 1 {
        unsafe { MmapedSafetensors::new(&base_paths[0]) }
    } else {
        unsafe { MmapedSafetensors::multi(&base_paths) }
    }
    .context("failed to mmap base safetensors")?;

    // 3. Load adapter tensors
    let adapter =
        candle_core::safetensors::load(&config.adapter_path, &Device::Cpu).with_context(|| {
            format!(
                "failed to load adapter from {}",
                config.adapter_path.display()
            )
        })?;

    // 4. Infer rank from any lora_a tensor
    let rank = adapter
        .iter()
        .find(|(k, _)| k.ends_with("lora_a"))
        .map(|(_, t)| t.dim(0))
        .context("no lora_a tensor found in adapter")?
        .context("failed to read lora_a dimension")?;

    // 5. Scale = alpha / rank = (rank * 2) / rank = 2.0
    let scale = 2.0_f64;

    // 6. Iterate base tensors and merge where adapters exist
    let base_tensor_info = base.tensors();
    let mut merged_tensors: HashMap<String, Tensor> = HashMap::new();
    let mut merge_layer_idx: usize = 0;

    // Count how many layers actually have adapters for progress reporting
    let total_merge_layers = {
        let mut count = 0usize;
        for i in 0..num_layers {
            let q_a = format!("model.layers.{i}.self_attn.q_proj.lora_a");
            let v_a = format!("model.layers.{i}.self_attn.v_proj.lora_a");
            if adapter.contains_key(&q_a) || adapter.contains_key(&v_a) {
                count += 1;
            }
        }
        count
    };

    // Track which layers we've already sent progress for
    let mut reported_layers: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (name, _tv) in &base_tensor_info {
        let base_tensor = base
            .load(name, &Device::Cpu)
            .with_context(|| format!("failed to load base tensor {name}"))?;
        let base_dtype = base_tensor.dtype();

        let stem = name.strip_suffix(".weight").unwrap_or(name);
        let lora_a_key = format!("{stem}.lora_a");
        let lora_b_key = format!("{stem}.lora_b");

        let tensor = if let (Some(lora_a), Some(lora_b)) =
            (adapter.get(&lora_a_key), adapter.get(&lora_b_key))
        {
            // Report progress per-layer
            // Extract layer index from the stem if possible
            if let Some(layer_num) = extract_layer_index(stem)
                && reported_layers.insert(layer_num)
            {
                merge_layer_idx += 1;
                let _ = tx.send(MergeProgress::Merging {
                    layer: merge_layer_idx,
                    total: total_merge_layers,
                });
            }

            let base_f32 = base_tensor.to_dtype(DType::F32)?;
            let a_f32 = lora_a.to_dtype(DType::F32)?;
            let b_f32 = lora_b.to_dtype(DType::F32)?;
            let delta = (b_f32.matmul(&a_f32)? * scale)?;
            let merged = (base_f32 + delta)?;
            merged.to_dtype(base_dtype)?
        } else {
            base_tensor
        };

        merged_tensors.insert(name.clone(), tensor);
    }

    // Ensure rank is used (suppress unused warning via a debug log)
    let _ = rank;

    // 7. Save merged model
    let _ = tx.send(MergeProgress::Saving);
    std::fs::create_dir_all(&config.output_dir).with_context(|| {
        format!(
            "failed to create output dir {}",
            config.output_dir.display()
        )
    })?;

    let output_model = config.output_dir.join("model.safetensors");
    candle_core::safetensors::save(&merged_tensors, &output_model)
        .with_context(|| format!("failed to save merged model to {}", output_model.display()))?;

    // 8. Copy config/tokenizer files (skip missing)
    let copy_files = [
        "config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "generation_config.json",
    ];
    for filename in &copy_files {
        let src = config.base_dir.join(filename);
        if src.exists() {
            let dst = config.output_dir.join(filename);
            if let Err(e) = std::fs::copy(&src, &dst) {
                eprintln!("[merge] warning: failed to copy {}: {e:#}", src.display());
            }
        }
    }

    // 9. Done
    let _ = tx.send(MergeProgress::Done {
        output_path: output_model,
    });
    Ok(())
}

/// Extract a numeric layer index from a tensor name stem such as
/// `model.layers.5.self_attn.q_proj`.
fn extract_layer_index(stem: &str) -> Option<usize> {
    let parts: Vec<&str> = stem.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if *part == "layers" {
            return parts.get(i + 1).and_then(|s| s.parse::<usize>().ok());
        }
    }
    None
}
