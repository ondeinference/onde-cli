//! GGUF writer module.
//!
//! Converts a merged safetensors model directory (containing `config.json`,
//! `tokenizer.json`, and `model.safetensors`) into a single `.gguf` file
//! compatible with mistral.rs / llama.cpp.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use candle_core::Device;
use candle_core::safetensors::MmapedSafetensors;
use half::f16;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Target data type for the exported GGUF tensors.
#[allow(dead_code)]
pub enum GgufDtype {
    F16,
    Q8_0,
}

/// Configuration for a GGUF export run.
pub struct GgufConfig {
    /// Directory containing `config.json`, `tokenizer.json`, and
    /// `model.safetensors`.
    pub model_dir: PathBuf,
    /// Path where the output `.gguf` file will be written.
    pub output_path: PathBuf,
    /// Target quantisation type for 2-D weight tensors.
    pub dtype: GgufDtype,
}

/// Progress events emitted by the background export thread.
pub enum GgufProgress {
    ReadingModel,
    WritingTensor {
        index: usize,
        total: usize,
        name: String,
    },
    Done {
        output_path: PathBuf,
        size_bytes: u64,
    },
    Failed(String),
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn start_gguf_export(config: GgufConfig, tx: tokio::sync::mpsc::UnboundedSender<GgufProgress>) {
    std::thread::spawn(move || {
        if let Err(e) = run_gguf_export(&config, &tx) {
            eprintln!("[gguf] error: {e:#}");
            let _ = tx.send(GgufProgress::Failed(format!("{e:#}")));
        }
    });
}

// ---------------------------------------------------------------------------
// Private — model / tokenizer config structs
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ModelConfig {
    hidden_size: usize,
    num_hidden_layers: usize,
    num_attention_heads: usize,
    num_key_value_heads: Option<usize>,
    /// Per-head attention dimension. Decoupled from `hidden_size / num_attention_heads`
    /// in Qwen3 (e.g. Qwen3-0.6B: hidden_size 1024, 16 heads, but head_dim 128).
    /// Absent in Qwen2/2.5 configs, where it equals `hidden_size / num_attention_heads`.
    #[serde(default)]
    head_dim: Option<usize>,
    intermediate_size: usize,
    vocab_size: usize,
    #[serde(default = "default_rms_norm_eps")]
    rms_norm_eps: f64,
    #[serde(default = "default_rope_theta")]
    rope_theta: f64,
    #[serde(default = "default_max_position_embeddings")]
    max_position_embeddings: usize,
    #[serde(default)]
    tie_word_embeddings: bool,
    /// HuggingFace model type, e.g. "qwen2" or "qwen3".
    /// Kept for future architecture-specific logic.
    #[allow(dead_code)]
    #[serde(default = "default_model_type")]
    model_type: String,
}

fn default_rms_norm_eps() -> f64 {
    1e-6
}
fn default_rope_theta() -> f64 {
    10000.0
}
fn default_max_position_embeddings() -> usize {
    32768
}
fn default_model_type() -> String {
    "qwen2".to_string()
}

#[derive(serde::Deserialize)]
struct TokenizerJson {
    model: TokenizerModel,
    #[serde(default)]
    added_tokens: Vec<AddedToken>,
}

#[derive(serde::Deserialize)]
struct TokenizerModel {
    vocab: HashMap<String, u32>,
    #[serde(default, deserialize_with = "deserialize_merges")]
    merges: Vec<String>,
}

/// A single merge entry — either `"a b"` (Qwen2.5) or `["a","b"]` (Qwen3).
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum MergeEntry {
    Str(String),
    Pair(Vec<String>),
}

/// Deserialize merges from either format into a uniform `Vec<String>` of
/// space-separated pairs (the format GGUF expects).
fn deserialize_merges<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let entries: Vec<MergeEntry> = Vec::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .map(|e| match e {
            MergeEntry::Str(s) => s,
            MergeEntry::Pair(parts) => parts.join(" "),
        })
        .collect())
}

#[derive(serde::Deserialize)]
struct AddedToken {
    id: u32,
    content: String,
    special: bool,
}

// ---------------------------------------------------------------------------
// GGUF constants
// ---------------------------------------------------------------------------

const GGUF_MAGIC: u32 = 0x46554747;
const GGUF_VERSION: u32 = 3;
const ALIGNMENT: u64 = 32;

/// Standard Qwen ChatML template used as a fallback when `tokenizer_config.json`
/// does not provide one. Matches the template shipped in Bartowski's Qwen GGUFs.
const QWEN_CHATML_TEMPLATE: &str = concat!(
    "{%- for message in messages %}",
    "{{'<|im_start|>' + message['role'] + '\n' + message['content'] + '<|im_end|>' + '\n'}}",
    "{%- endfor %}",
    "{%- if add_generation_prompt %}",
    "{{'<|im_start|>assistant\n'}}",
    "{%- endif %}",
);

// GGUF tensor type IDs
const GGUF_TYPE_F32: u32 = 0;
const GGUF_TYPE_F16: u32 = 1;
const GGUF_TYPE_Q8_0: u32 = 8;

// GGUF metadata value type IDs
const GGUF_META_U32: u32 = 4;
const GGUF_META_INT32: u32 = 5;
const GGUF_META_F32: u32 = 6;
const GGUF_META_STRING: u32 = 8;
const GGUF_META_ARRAY: u32 = 9;

// ---------------------------------------------------------------------------
// Private — GGUF write helpers
// ---------------------------------------------------------------------------

fn write_u32(w: &mut impl Write, v: u32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_u64(w: &mut impl Write, v: u64) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_f32(w: &mut impl Write, v: f32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_gguf_string(w: &mut impl Write, s: &str) -> std::io::Result<()> {
    write_u64(w, s.len() as u64)?;
    w.write_all(s.as_bytes())
}

fn write_kv_string(w: &mut impl Write, key: &str, val: &str) -> std::io::Result<()> {
    write_gguf_string(w, key)?;
    write_u32(w, GGUF_META_STRING)?;
    write_gguf_string(w, val)
}

fn write_kv_u32(w: &mut impl Write, key: &str, val: u32) -> std::io::Result<()> {
    write_gguf_string(w, key)?;
    write_u32(w, GGUF_META_U32)?;
    write_u32(w, val)
}

fn write_kv_f32(w: &mut impl Write, key: &str, val: f32) -> std::io::Result<()> {
    write_gguf_string(w, key)?;
    write_u32(w, GGUF_META_F32)?;
    write_f32(w, val)
}

fn write_kv_string_array(w: &mut impl Write, key: &str, vals: &[String]) -> std::io::Result<()> {
    write_gguf_string(w, key)?;
    write_u32(w, GGUF_META_ARRAY)?;
    write_u32(w, GGUF_META_STRING)?;
    write_u64(w, vals.len() as u64)?;
    for s in vals {
        write_gguf_string(w, s)?;
    }
    Ok(())
}

/// Write an array of signed 32-bit integers. GGUF declares `token_type` as an
/// INT32 array; llama.cpp / mistral.rs read it with `to_i32()` and reject a
/// UINT32-typed array, so the element type tag must be INT32 (not U32).
fn write_kv_i32_array(w: &mut impl Write, key: &str, vals: &[i32]) -> std::io::Result<()> {
    write_gguf_string(w, key)?;
    write_u32(w, GGUF_META_ARRAY)?;
    write_u32(w, GGUF_META_INT32)?;
    write_u64(w, vals.len() as u64)?;
    for &v in vals {
        w.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

fn pad_to_alignment(w: &mut impl Write, pos: u64, alignment: u64) -> std::io::Result<u64> {
    let rem = pos % alignment;
    if rem != 0 {
        let pad = alignment - rem;
        for _ in 0..pad {
            w.write_all(&[0u8])?;
        }
        Ok(pos + pad)
    } else {
        Ok(pos)
    }
}

// ---------------------------------------------------------------------------
// Private — tensor name mapping (HuggingFace → GGUF)
// ---------------------------------------------------------------------------

fn map_tensor_name(hf_name: &str) -> Option<String> {
    // Non-layer names
    if hf_name == "model.embed_tokens.weight" {
        return Some("token_embd.weight".to_string());
    }
    if hf_name == "model.norm.weight" {
        return Some("output_norm.weight".to_string());
    }
    if hf_name == "lm_head.weight" {
        return Some("output.weight".to_string());
    }

    // Layer names: model.layers.{i}.suffix → blk.{i}.gguf_suffix
    let prefix = "model.layers.";
    if !hf_name.starts_with(prefix) {
        return None;
    }

    let rest = &hf_name[prefix.len()..];
    let dot_pos = rest.find('.')?;
    let layer_idx = &rest[..dot_pos];
    let suffix = &rest[dot_pos + 1..];

    let gguf_suffix = match suffix {
        "self_attn.q_proj.weight" => "attn_q.weight",
        "self_attn.k_proj.weight" => "attn_k.weight",
        "self_attn.v_proj.weight" => "attn_v.weight",
        "self_attn.o_proj.weight" => "attn_output.weight",
        // Bias tensors (present in Qwen2/2.5, absent in Qwen3)
        "self_attn.q_proj.bias" => "attn_q.bias",
        "self_attn.k_proj.bias" => "attn_k.bias",
        "self_attn.v_proj.bias" => "attn_v.bias",
        // QK-Norm (Qwen3)
        "self_attn.q_norm.weight" => "attn_q_norm.weight",
        "self_attn.k_norm.weight" => "attn_k_norm.weight",
        "mlp.gate_proj.weight" => "ffn_gate.weight",
        "mlp.up_proj.weight" => "ffn_up.weight",
        "mlp.down_proj.weight" => "ffn_down.weight",
        "input_layernorm.weight" => "attn_norm.weight",
        "post_attention_layernorm.weight" => "ffn_norm.weight",
        _ => return None,
    };

    Some(format!("blk.{layer_idx}.{gguf_suffix}"))
}

// ---------------------------------------------------------------------------
// Private — tensor data size calculation
// ---------------------------------------------------------------------------

fn tensor_data_size(num_elements: usize, gguf_type: u32) -> usize {
    match gguf_type {
        GGUF_TYPE_F32 => num_elements * 4,
        GGUF_TYPE_F16 => num_elements * 2,
        GGUF_TYPE_Q8_0 => num_elements.div_ceil(32) * 34,
        _ => num_elements * 4,
    }
}

// ---------------------------------------------------------------------------
// Private — determine GGUF type for a tensor
// ---------------------------------------------------------------------------

fn choose_gguf_type(gguf_name: &str, ndim: usize, dtype: &GgufDtype) -> u32 {
    // 1-D tensors (norms, biases) → always F32
    if ndim <= 1 {
        return GGUF_TYPE_F32;
    }

    // Embedding and output head → always F16
    if gguf_name == "token_embd.weight" || gguf_name == "output.weight" {
        return GGUF_TYPE_F16;
    }

    // 2-D weight tensors → follow requested dtype
    match dtype {
        GgufDtype::F16 => GGUF_TYPE_F16,
        GgufDtype::Q8_0 => GGUF_TYPE_Q8_0,
    }
}

// ---------------------------------------------------------------------------
// Private — tensor data conversion and writing
// ---------------------------------------------------------------------------

fn write_tensor_f32(w: &mut impl Write, data: &[f32]) -> std::io::Result<()> {
    for &v in data {
        w.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

fn write_tensor_f16(w: &mut impl Write, data: &[f32]) -> std::io::Result<()> {
    for &v in data {
        w.write_all(&f16::from_f32(v).to_le_bytes())?;
    }
    Ok(())
}

fn write_tensor_q8_0(w: &mut impl Write, data: &[f32]) -> std::io::Result<()> {
    let block_size = 32;
    let num_blocks = data.len().div_ceil(block_size);

    for block_idx in 0..num_blocks {
        let start = block_idx * block_size;
        let end = (start + block_size).min(data.len());
        let block = &data[start..end];

        // Find max absolute value in the block
        let mut amax: f32 = 0.0;
        for &v in block {
            let abs = v.abs();
            if abs > amax {
                amax = abs;
            }
        }

        let scale = if amax == 0.0 { 0.0 } else { amax / 127.0 };
        let inv_scale = if scale == 0.0 { 0.0 } else { 1.0 / scale };

        // Write scale as f16 (2 bytes)
        w.write_all(&f16::from_f32(scale).to_le_bytes())?;

        // Quantize and write 32 i8 values
        for i in 0..block_size {
            let val = if i < block.len() { block[i] } else { 0.0 };
            let q = (val * inv_scale).round().clamp(-128.0, 127.0) as i8;
            w.write_all(&[q as u8])?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Private — compute byte sizes of metadata
// ---------------------------------------------------------------------------

/// Compute the byte size of a GGUF string (length prefix + bytes).
fn gguf_string_size(s: &str) -> u64 {
    8 + s.len() as u64
}

/// Compute the byte size of a KV string entry.
fn kv_string_size(key: &str, val: &str) -> u64 {
    gguf_string_size(key) + 4 + gguf_string_size(val)
}

/// Compute the byte size of a KV u32 entry.
fn kv_u32_size(key: &str) -> u64 {
    gguf_string_size(key) + 4 + 4
}

/// Compute the byte size of a KV f32 entry.
fn kv_f32_size(key: &str) -> u64 {
    gguf_string_size(key) + 4 + 4
}

/// Compute the byte size of a KV string-array entry.
fn kv_string_array_size(key: &str, vals: &[String]) -> u64 {
    let mut size = gguf_string_size(key) + 4; // key + value_type
    size += 4 + 8; // array element_type + count
    for s in vals {
        size += gguf_string_size(s);
    }
    size
}

/// Compute the byte size of a KV i32-array entry (same layout as u32).
fn kv_i32_array_size(key: &str, vals: &[i32]) -> u64 {
    gguf_string_size(key) + 4 + 4 + 8 + (vals.len() as u64 * 4)
}

/// Compute the byte size of a single tensor info entry.
fn tensor_info_size(name: &str, ndim: usize) -> u64 {
    // name_len(8) + name_bytes + n_dims(4) + dims(8*ndim) + type(4) + offset(8)
    8 + name.len() as u64 + 4 + (ndim as u64 * 8) + 4 + 8
}

// ---------------------------------------------------------------------------
// Private — core export logic
// ---------------------------------------------------------------------------

/// Holds the info we need for each tensor in the GGUF file.
struct TensorEntry {
    /// GGUF tensor name.
    gguf_name: String,
    /// Original HuggingFace tensor name (for loading from safetensors).
    hf_name: String,
    /// Dimensions in GGUF order (row-major, i.e. same order as safetensors).
    dims: Vec<u64>,
    /// Number of dimensions.
    ndim: usize,
    /// Total number of elements.
    num_elements: usize,
    /// GGUF tensor type ID.
    gguf_type: u32,
    /// Size in bytes of the tensor data.
    data_size: usize,
    /// Offset from start of data section.
    offset: u64,
}

fn run_gguf_export(
    config: &GgufConfig,
    tx: &tokio::sync::mpsc::UnboundedSender<GgufProgress>,
) -> Result<()> {
    let _ = tx.send(GgufProgress::ReadingModel);

    // -----------------------------------------------------------------------
    // 1. Load model config
    // -----------------------------------------------------------------------
    let config_path = config.model_dir.join("config.json");
    let config_text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let model_cfg: ModelConfig =
        serde_json::from_str(&config_text).context("failed to parse config.json")?;

    // -----------------------------------------------------------------------
    // 2. Load and parse tokenizer.json
    // -----------------------------------------------------------------------
    let tok_path = config.model_dir.join("tokenizer.json");
    let tok_text = std::fs::read_to_string(&tok_path)
        .with_context(|| format!("failed to read {}", tok_path.display()))?;
    let tok: TokenizerJson =
        serde_json::from_str(&tok_text).context("failed to parse tokenizer.json")?;

    // Read the chat template from tokenizer_config.json.
    // This is needed by mistral.rs / llama.cpp to format multi-turn conversations.
    //
    // The field can be either:
    //   - a plain string (Qwen2/2.5)
    //   - an array of {"name": "...", "template": "..."} objects (Qwen3)
    //
    // When it's an array, pick the "default" entry, or fall back to the first.
    // If the file is missing or the field is absent, use the standard Qwen
    // ChatML template so the GGUF is always loadable for chat.
    let chat_template: String = {
        let tok_config_path = config.model_dir.join("tokenizer_config.json");
        let from_file = std::fs::read_to_string(&tok_config_path)
            .ok()
            .and_then(|text| {
                let v: serde_json::Value = serde_json::from_str(&text).ok()?;
                let ct = v.get("chat_template")?;

                // Plain string
                if let Some(s) = ct.as_str() {
                    return Some(s.to_string());
                }

                // Array of objects — pick "default", else first entry
                if let Some(arr) = ct.as_array() {
                    let default = arr.iter().find(|entry| {
                        entry.get("name").and_then(|n| n.as_str()) == Some("default")
                    });
                    let chosen = default.or_else(|| arr.first());
                    return chosen
                        .and_then(|entry| entry.get("template"))
                        .and_then(|t| t.as_str())
                        .map(String::from);
                }

                None
            });

        from_file.unwrap_or_else(|| QWEN_CHATML_TEMPLATE.to_string())
    };

    // Build ordered token list sorted by ID
    let vocab_size = model_cfg.vocab_size;
    let mut id_to_token: Vec<(u32, String)> =
        tok.model.vocab.into_iter().map(|(k, v)| (v, k)).collect();
    // Also include added_tokens that might not be in the base vocab
    for at in &tok.added_tokens {
        if !id_to_token.iter().any(|(id, _)| *id == at.id) {
            id_to_token.push((at.id, at.content.clone()));
        }
    }
    id_to_token.sort_by_key(|(id, _)| *id);

    // Pad to vocab_size if needed
    let mut tokens: Vec<String> = Vec::with_capacity(vocab_size);
    let mut next_expected: u32 = 0;
    for (id, content) in &id_to_token {
        while next_expected < *id && (next_expected as usize) < vocab_size {
            tokens.push(format!("<unused{next_expected}>"));
            next_expected += 1;
        }
        if (next_expected as usize) < vocab_size {
            tokens.push(content.clone());
            next_expected += 1;
        }
    }
    while tokens.len() < vocab_size {
        let idx = tokens.len();
        tokens.push(format!("<unused{idx}>"));
    }
    tokens.truncate(vocab_size);

    // Build special token lookup
    let special_set: HashMap<u32, bool> = tok
        .added_tokens
        .iter()
        .filter(|at| at.special)
        .map(|at| (at.id, true))
        .collect();

    // Token types: 1 = normal, 3 = control (special). Stored as INT32 in GGUF.
    let token_types: Vec<i32> = (0..vocab_size as u32)
        .map(|id| if special_set.contains_key(&id) { 3 } else { 1 })
        .collect();

    // BOS / EOS token IDs
    let added_token_id = |name: &str| -> Option<u32> {
        tok.added_tokens
            .iter()
            .find(|at| at.content == name)
            .map(|at| at.id)
    };
    let bos_id = added_token_id("<|endoftext|>").unwrap_or(0);
    let eos_id = added_token_id("<|im_end|>")
        .or_else(|| added_token_id("<|endoftext|>"))
        .unwrap_or(0);

    let merges = tok.model.merges;

    // -----------------------------------------------------------------------
    // 3. Load safetensors
    // -----------------------------------------------------------------------
    let st_path = config.model_dir.join("model.safetensors");
    // SAFETY: the file is memory-mapped read-only.
    let safetensors = unsafe { MmapedSafetensors::new(&st_path) }
        .with_context(|| format!("failed to mmap {}", st_path.display()))?;
    let tensor_list = safetensors.tensors();

    // -----------------------------------------------------------------------
    // 4. Build tensor entries with name mapping
    // -----------------------------------------------------------------------
    let mut entries: Vec<TensorEntry> = Vec::new();
    for (hf_name, tensor_view) in &tensor_list {
        let gguf_name = match map_tensor_name(hf_name) {
            Some(n) => n,
            None => continue,
        };

        let shape = tensor_view.shape();
        let ndim = shape.len();
        let num_elements: usize = shape.iter().product();
        // GGUF stores dimensions innermost-first (llama.cpp convention); the
        // reader reverses them back to logical order. safetensors shapes are
        // outermost-first, so write them reversed. For a Linear weight stored
        // as [out, in] this yields ne = [in, out], matching what mistral.rs /
        // candle expect after their `dimensions.reverse()`.
        let dims: Vec<u64> = shape.iter().rev().map(|&d| d as u64).collect();
        let gguf_type = choose_gguf_type(&gguf_name, ndim, &config.dtype);
        let data_size = tensor_data_size(num_elements, gguf_type);

        entries.push(TensorEntry {
            gguf_name,
            hf_name: hf_name.clone(),
            dims,
            ndim,
            num_elements,
            gguf_type,
            data_size,
            offset: 0, // computed below
        });
    }

    // Handle tie_word_embeddings: if lm_head.weight is missing, duplicate
    // embed_tokens as output.weight
    let has_output = entries.iter().any(|e| e.gguf_name == "output.weight");
    if !has_output
        && model_cfg.tie_word_embeddings
        && let Some(embd) = entries.iter().find(|e| e.gguf_name == "token_embd.weight")
    {
        let gguf_type = choose_gguf_type("output.weight", embd.ndim, &config.dtype);
        let data_size = tensor_data_size(embd.num_elements, gguf_type);
        entries.push(TensorEntry {
            gguf_name: "output.weight".to_string(),
            hf_name: "model.embed_tokens.weight".to_string(),
            dims: embd.dims.clone(),
            ndim: embd.ndim,
            num_elements: embd.num_elements,
            gguf_type,
            data_size,
            offset: 0,
        });
    }

    // Synthesize zero-filled Q/K/V bias tensors for layers that lack them.
    //
    // mistralrs expects `blk.{i}.attn_q.bias` etc. for the qwen2 architecture.
    // Qwen2/2.5 have these in the safetensors; Qwen3 does not. Bartowski's
    // llama.cpp-converted Qwen3 GGUFs include zero-filled bias tensors.
    // We do the same here so the GGUF loads correctly for both model families.
    let num_layers = model_cfg.num_hidden_layers;
    let num_heads = model_cfg.num_attention_heads;
    let num_kv_heads_usize = model_cfg.num_key_value_heads.unwrap_or(num_heads);
    // Qwen3 decouples head_dim from hidden_size / num_heads; honour the explicit
    // config value when present so synthesized bias tensors and the GGUF
    // key_length/value_length metadata match the real attention dimension.
    let head_dim = model_cfg
        .head_dim
        .unwrap_or(model_cfg.hidden_size / num_heads);

    for i in 0..num_layers {
        let bias_specs: &[(&str, usize)] = &[
            (&format!("blk.{i}.attn_q.bias"), num_heads * head_dim),
            (
                &format!("blk.{i}.attn_k.bias"),
                num_kv_heads_usize * head_dim,
            ),
            (
                &format!("blk.{i}.attn_v.bias"),
                num_kv_heads_usize * head_dim,
            ),
        ];
        for (gguf_name, num_elements) in bias_specs {
            if !entries.iter().any(|e| e.gguf_name == *gguf_name) {
                // 1-D bias → always F32
                let data_size = tensor_data_size(*num_elements, GGUF_TYPE_F32);
                entries.push(TensorEntry {
                    gguf_name: gguf_name.to_string(),
                    hf_name: String::new(), // sentinel: no safetensors source
                    dims: vec![*num_elements as u64],
                    ndim: 1,
                    num_elements: *num_elements,
                    gguf_type: GGUF_TYPE_F32,
                    data_size,
                    offset: 0,
                });
            }
        }
    }

    // Compute offsets (relative to start of data section, 32-byte aligned)
    let mut data_offset: u64 = 0;
    for entry in &mut entries {
        entry.offset = data_offset;
        let size = entry.data_size as u64;
        data_offset += size;
        // Pad to 32-byte alignment
        let rem = data_offset % ALIGNMENT;
        if rem != 0 {
            data_offset += ALIGNMENT - rem;
        }
    }

    // -----------------------------------------------------------------------
    // 5. Compute header size to know where data section starts
    // -----------------------------------------------------------------------
    let num_tensors = entries.len() as u64;
    let num_kv_heads = model_cfg
        .num_key_value_heads
        .unwrap_or(model_cfg.num_attention_heads) as u32;
    let file_type_val: u32 = match config.dtype {
        GgufDtype::F16 => 1,
        GgufDtype::Q8_0 => 7,
    };

    // GGUF architecture string.
    //
    // Qwen3 has the same tensor layout as Qwen2 plus mandatory QK-norm
    // (`attn_q_norm` / `attn_k_norm`). mistral.rs ships dedicated loaders for
    // each — `quantized_qwen` for `qwen2`, `quantized_qwen3` for `qwen3` — so
    // declare the architecture that matches the source `model_type`. Routing a
    // Qwen3 model through the qwen2 loader loads but produces garbage output.
    let arch = if model_cfg.model_type.starts_with("qwen3") {
        "qwen3"
    } else {
        "qwen2"
    };

    // Count metadata KV entries
    let num_metadata: u64 = 20;

    // Pre-compute header + metadata + tensor info byte size
    let mut header_size: u64 = 0;
    // Magic + version + tensor_count + metadata_kv_count
    header_size += 4 + 4 + 8 + 8;

    // Metadata sizes
    header_size += kv_string_size("general.architecture", arch);
    header_size += kv_string_size("general.name", "onde-finetuned");
    header_size += kv_u32_size("general.file_type");
    header_size += kv_u32_size(&format!("{arch}.block_count"));
    header_size += kv_u32_size(&format!("{arch}.embedding_length"));
    header_size += kv_u32_size(&format!("{arch}.feed_forward_length"));
    header_size += kv_u32_size(&format!("{arch}.attention.head_count"));
    header_size += kv_u32_size(&format!("{arch}.attention.head_count_kv"));
    header_size += kv_u32_size(&format!("{arch}.attention.key_length"));
    header_size += kv_u32_size(&format!("{arch}.attention.value_length"));
    header_size += kv_f32_size(&format!("{arch}.attention.layer_norm_rms_epsilon"));
    header_size += kv_f32_size(&format!("{arch}.rope.freq_base"));
    header_size += kv_u32_size(&format!("{arch}.context_length"));
    header_size += kv_string_size("tokenizer.ggml.model", "gpt2");
    header_size += kv_string_array_size("tokenizer.ggml.tokens", &tokens);
    header_size += kv_i32_array_size("tokenizer.ggml.token_type", &token_types);
    header_size += kv_string_array_size("tokenizer.ggml.merges", &merges);
    header_size += kv_u32_size("tokenizer.ggml.bos_token_id");
    header_size += kv_u32_size("tokenizer.ggml.eos_token_id");
    header_size += kv_string_size("tokenizer.chat_template", &chat_template);

    // Tensor info sizes
    for entry in &entries {
        header_size += tensor_info_size(&entry.gguf_name, entry.ndim);
    }

    // Pad header to alignment
    let header_padded = {
        let rem = header_size % ALIGNMENT;
        if rem != 0 {
            header_size + (ALIGNMENT - rem)
        } else {
            header_size
        }
    };

    // -----------------------------------------------------------------------
    // 6. Write the GGUF file
    // -----------------------------------------------------------------------
    let file = std::fs::File::create(&config.output_path)
        .with_context(|| format!("failed to create {}", config.output_path.display()))?;
    let mut w = BufWriter::new(file);

    // Header
    write_u32(&mut w, GGUF_MAGIC)?;
    write_u32(&mut w, GGUF_VERSION)?;
    write_u64(&mut w, num_tensors)?;
    write_u64(&mut w, num_metadata)?;

    // Metadata KV pairs
    write_kv_string(&mut w, "general.architecture", arch)?;
    write_kv_string(&mut w, "general.name", "onde-finetuned")?;
    write_kv_u32(&mut w, "general.file_type", file_type_val)?;
    write_kv_u32(
        &mut w,
        &format!("{arch}.block_count"),
        model_cfg.num_hidden_layers as u32,
    )?;
    write_kv_u32(
        &mut w,
        &format!("{arch}.embedding_length"),
        model_cfg.hidden_size as u32,
    )?;
    write_kv_u32(
        &mut w,
        &format!("{arch}.feed_forward_length"),
        model_cfg.intermediate_size as u32,
    )?;
    write_kv_u32(
        &mut w,
        &format!("{arch}.attention.head_count"),
        model_cfg.num_attention_heads as u32,
    )?;
    write_kv_u32(
        &mut w,
        &format!("{arch}.attention.head_count_kv"),
        num_kv_heads,
    )?;
    // Per-head dimension. mistral.rs / llama.cpp set head_dim = key_length and
    // fall back to embedding_length / head_count when these keys are absent —
    // which is wrong for Qwen3 (head_dim is decoupled), so always emit them.
    write_kv_u32(
        &mut w,
        &format!("{arch}.attention.key_length"),
        head_dim as u32,
    )?;
    write_kv_u32(
        &mut w,
        &format!("{arch}.attention.value_length"),
        head_dim as u32,
    )?;
    write_kv_f32(
        &mut w,
        &format!("{arch}.attention.layer_norm_rms_epsilon"),
        model_cfg.rms_norm_eps as f32,
    )?;
    write_kv_f32(
        &mut w,
        &format!("{arch}.rope.freq_base"),
        model_cfg.rope_theta as f32,
    )?;
    write_kv_u32(
        &mut w,
        &format!("{arch}.context_length"),
        model_cfg.max_position_embeddings as u32,
    )?;
    write_kv_string(&mut w, "tokenizer.ggml.model", "gpt2")?;
    write_kv_string_array(&mut w, "tokenizer.ggml.tokens", &tokens)?;
    write_kv_i32_array(&mut w, "tokenizer.ggml.token_type", &token_types)?;
    write_kv_string_array(&mut w, "tokenizer.ggml.merges", &merges)?;
    write_kv_u32(&mut w, "tokenizer.ggml.bos_token_id", bos_id)?;
    write_kv_u32(&mut w, "tokenizer.ggml.eos_token_id", eos_id)?;
    write_kv_string(&mut w, "tokenizer.chat_template", &chat_template)?;

    // Tensor info entries
    for entry in &entries {
        write_gguf_string(&mut w, &entry.gguf_name)?;
        write_u32(&mut w, entry.ndim as u32)?;
        for &d in &entry.dims {
            write_u64(&mut w, d)?;
        }
        write_u32(&mut w, entry.gguf_type)?;
        write_u64(&mut w, entry.offset)?;
    }

    // Pad to alignment before data section
    let mut pos = header_size;
    pos = pad_to_alignment(&mut w, pos, ALIGNMENT)?;
    let _ = header_padded; // should match pos

    // -----------------------------------------------------------------------
    // 7. Write tensor data
    // -----------------------------------------------------------------------
    let total = entries.len();
    for (idx, entry) in entries.iter().enumerate() {
        let _ = tx.send(GgufProgress::WritingTensor {
            index: idx + 1,
            total,
            name: entry.gguf_name.clone(),
        });

        // Load tensor data. Synthetic entries (empty hf_name) get zeros.
        let data: Vec<f32> = if entry.hf_name.is_empty() {
            vec![0.0f32; entry.num_elements]
        } else {
            let tensor = safetensors
                .load(&entry.hf_name, &Device::Cpu)
                .with_context(|| format!("failed to load tensor {}", entry.hf_name))?;
            let tensor_f32 = tensor
                .to_dtype(candle_core::DType::F32)
                .with_context(|| format!("failed to convert {} to f32", entry.hf_name))?;
            let flat = tensor_f32
                .flatten_all()
                .context("failed to flatten tensor")?;
            flat.to_vec1::<f32>()
                .context("failed to extract f32 data from tensor")?
        };

        match entry.gguf_type {
            GGUF_TYPE_F32 => write_tensor_f32(&mut w, &data)?,
            GGUF_TYPE_F16 => write_tensor_f16(&mut w, &data)?,
            GGUF_TYPE_Q8_0 => write_tensor_q8_0(&mut w, &data)?,
            _ => write_tensor_f32(&mut w, &data)?,
        }

        pos += entry.data_size as u64;

        // Pad to alignment (except possibly the last tensor)
        if idx + 1 < total {
            pos = pad_to_alignment(&mut w, pos, ALIGNMENT)?;
        }
    }

    w.flush().context("failed to flush GGUF output")?;
    drop(w);

    // -----------------------------------------------------------------------
    // 8. Report completion
    // -----------------------------------------------------------------------
    let size_bytes = std::fs::metadata(&config.output_path)
        .with_context(|| format!("failed to stat {}", config.output_path.display()))?
        .len();

    let _ = tx.send(GgufProgress::Done {
        output_path: config.output_path.clone(),
        size_bytes,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve the locally cached merged Qwen3-0.6B model directory, if present.
    /// Returns `None` so the heavy integration test can be skipped on machines
    /// that have not run a fine-tune.
    fn merged_qwen3_dir() -> Option<PathBuf> {
        let dir = dirs::home_dir()?
            .join("Library/Group Containers/group.com.ondeinference.apps")
            .join("models/hub/models--Qwen--Qwen3-0.6B/snapshots/merged-model");
        if dir.join("config.json").exists() && dir.join("model.safetensors").exists() {
            Some(dir)
        } else {
            None
        }
    }

    /// Resolve the cached base Qwen3-0.6B snapshot (the hex commit dir), if present.
    fn base_qwen3_dir() -> Option<PathBuf> {
        let snapshots = dirs::home_dir()?
            .join("Library/Group Containers/group.com.ondeinference.apps")
            .join("models/hub/models--Qwen--Qwen3-0.6B/snapshots");
        let entry = std::fs::read_dir(&snapshots).ok()?.flatten().find(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.len() >= 7 && n.chars().all(|c| c.is_ascii_hexdigit())
        })?;
        let dir = entry.path();
        (dir.join("config.json").exists() && dir.join("model.safetensors").exists()).then_some(dir)
    }

    /// Load a GGUF and generate a reply via the same `onde::mistralrs` engine the
    /// Onde SDK uses. Returns the assistant text. Panics on model/inference error.
    fn run_gguf_inference(gguf_path: &std::path::Path, user: &str) -> String {
        use onde::mistralrs::{
            GgufModelBuilder, RequestBuilder, Response, TextMessageRole, TokenSource,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let path = gguf_path.to_path_buf();
        let user = user.to_string();
        rt.block_on(async move {
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let model_root = path.parent().unwrap().to_string_lossy().to_string();
            let model = GgufModelBuilder::new(&model_root, vec![file_name])
                .with_token_source(TokenSource::None)
                .build()
                .await
                .expect("exported GGUF must load in mistral.rs");
            let mut req = RequestBuilder::new().set_sampler_max_len(512);
            if let Ok(system) = std::env::var("ONDE_TEST_SYSTEM_PROMPT") {
                req = req.add_message(TextMessageRole::System, &system);
            }
            let req = req.add_message(TextMessageRole::User, &user);
            let mut stream = model
                .stream_chat_request(req)
                .await
                .expect("inference request must start");
            let mut out = String::new();
            while let Some(resp) = stream.next().await {
                match resp {
                    Response::Chunk(chunk) => {
                        if let Some(choice) = chunk.choices.first()
                            && let Some(delta) = &choice.delta.content
                        {
                            out.push_str(delta);
                        }
                    }
                    Response::Done(_) => break,
                    Response::ModelError(msg, _) => panic!("model error: {msg}"),
                    Response::InternalError(e) => panic!("internal error: {e}"),
                    Response::ValidationError(e) => panic!("validation error: {e}"),
                    _ => {}
                }
            }
            out
        })
    }

    /// Full v1 pipeline regression guard: fine-tune Qwen3-0.6B → merge the LoRA
    /// adapter → export GGUF → load and run in mistral.rs. Proves the whole
    /// fine-tune → quantize → run path produces a valid, runnable model.
    ///
    /// Ignored by default (trains + loads ~1 GB on Metal/CPU); run with
    /// `cargo test gguf::tests::finetune_merge_export_run -- --ignored --nocapture`.
    #[test]
    #[ignore = "heavy: fine-tunes, merges, exports ~1GB GGUF and runs it"]
    fn finetune_merge_export_run() {
        let Some(base_dir) = base_qwen3_dir() else {
            eprintln!("base Qwen3-0.6B not found locally; skipping");
            return;
        };
        let data_path = match std::env::var("ONDE_TEST_DATA_PATH") {
            Ok(p) => PathBuf::from(p),
            Err(_) => dirs::home_dir()
                .unwrap()
                .join(".onde/datasets/Qwen/Qwen3-0.6B/train.jsonl"),
        };
        if !data_path.exists() {
            eprintln!("training data {} not found; skipping", data_path.display());
            return;
        }

        let work = std::env::temp_dir().join("onde-e2e-test");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();

        // 1. Fine-tune.
        let lora_dir = work.join("lora");
        let (ft_tx, mut ft_rx) = tokio::sync::mpsc::unbounded_channel();
        crate::finetune::start_finetune(
            crate::finetune::FineTuneConfig {
                model_dir: base_dir.clone(),
                data_path,
                output_dir: lora_dir.clone(),
                lora_rank: 8,
                lora_alpha: 16.0,
                learning_rate: std::env::var("ONDE_TEST_LR")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1e-4),
                epochs: std::env::var("ONDE_TEST_EPOCHS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(2),
                max_seq_len: std::env::var("ONDE_TEST_MAX_SEQ_LEN")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(512),
            },
            ft_tx,
        );
        let mut adapter_path = None;
        let mut last_loss = f32::NAN;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            while let Some(p) = ft_rx.recv().await {
                match p {
                    crate::finetune::FineTuneProgress::Training {
                        loss,
                        step,
                        total_steps,
                        epoch,
                        ..
                    } => {
                        eprintln!("[e2e] epoch {epoch} step {step}/{total_steps} loss {loss:.4}");
                        last_loss = loss;
                    }
                    crate::finetune::FineTuneProgress::Done { adapter_path: a } => {
                        adapter_path = Some(a);
                        break;
                    }
                    crate::finetune::FineTuneProgress::Failed(e) => panic!("fine-tune failed: {e}"),
                    _ => {}
                }
            }
        });
        let adapter_path = adapter_path.expect("fine-tune must produce an adapter");
        eprintln!("[e2e] fine-tune done, last loss {last_loss}");
        assert!(
            last_loss.is_finite(),
            "training loss is NaN/Inf — training diverged"
        );

        // 2. Merge.
        let merged_dir = work.join("merged");
        let (mg_tx, mut mg_rx) = tokio::sync::mpsc::unbounded_channel();
        crate::merge::start_merge(
            crate::merge::MergeConfig {
                base_dir,
                adapter_path,
                output_dir: merged_dir.clone(),
            },
            mg_tx,
        );
        let mut merged_ok = false;
        rt.block_on(async {
            while let Some(p) = mg_rx.recv().await {
                match p {
                    crate::merge::MergeProgress::Done { .. } => {
                        merged_ok = true;
                        break;
                    }
                    crate::merge::MergeProgress::Failed(e) => panic!("merge failed: {e}"),
                    _ => {}
                }
            }
        });
        assert!(merged_ok, "merge did not complete");

        // 3. Export GGUF (the dtype the CLI ships).
        let gguf_path = work.join("finetuned-q8_0.gguf");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        run_gguf_export(
            &GgufConfig {
                model_dir: merged_dir,
                output_path: gguf_path.clone(),
                dtype: GgufDtype::Q8_0,
            },
            &tx,
        )
        .expect("GGUF export should succeed");

        // 4. Load + run in mistral.rs. Query in the training distribution so a
        //    lightly fine-tuned model has something to say. `ONDE_TEST_EVAL_PROMPT`
        //    overrides the default (multiple prompts separated by `||`).
        let eval_prompts = std::env::var("ONDE_TEST_EVAL_PROMPT")
            .unwrap_or_else(|_| "Apa bahasa Jawa dari \"terima kasih\"?".to_string());
        for prompt in eval_prompts.split("||") {
            let reply = run_gguf_inference(&gguf_path, prompt.trim());
            eprintln!("[e2e] eval prompt: {prompt:?}");
            eprintln!("[e2e] fine-tuned GGUF reply: {reply:?}");
            assert!(
                !reply.trim().is_empty(),
                "fine-tuned GGUF produced an empty reply — not runnable"
            );
        }
    }

    /// End-to-end check: export a Qwen3 safetensors model to GGUF, then load and
    /// run it through the same `onde::mistralrs` engine the Onde SDK uses.
    ///
    /// This is the regression guard for the Qwen3 `head_dim` fix: with the wrong
    /// per-head dimension the GGUF loads with a reshape mismatch and inference
    /// fails. Ignored by default (downloads/loads ~1 GB, needs Metal/CPU);
    /// run with `cargo test gguf -- --ignored --nocapture`.
    #[test]
    #[ignore = "heavy: exports ~1GB GGUF and loads it in mistral.rs"]
    fn exported_qwen3_gguf_is_runnable() {
        // Allow overriding the source model dir and dtype to compare
        // base-vs-merged and F16-vs-Q8_0 without changing code.
        let model_dir = match std::env::var("ONDE_TEST_MODEL_DIR") {
            Ok(d) => PathBuf::from(d),
            // Default to the known-good base snapshot so this isolates the
            // exporter; `finetune_merge_export_run` covers the fine-tuned path.
            Err(_) => match base_qwen3_dir().or_else(merged_qwen3_dir) {
                Some(d) => d,
                None => {
                    eprintln!("Qwen3-0.6B model not found locally; skipping");
                    return;
                }
            },
        };
        let dtype = match std::env::var("ONDE_TEST_DTYPE").as_deref() {
            Ok("f16") => GgufDtype::F16,
            _ => GgufDtype::Q8_0,
        };
        eprintln!(
            "[test] model_dir={} dtype={}",
            model_dir.display(),
            if matches!(dtype, GgufDtype::F16) {
                "f16"
            } else {
                "q8_0"
            }
        );

        let out_dir = std::env::temp_dir().join("onde-gguf-test");
        std::fs::create_dir_all(&out_dir).unwrap();
        let output_path = out_dir.join("qwen3-0.6b-test.gguf");
        let _ = std::fs::remove_file(&output_path);

        // 1. Export GGUF.
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        run_gguf_export(
            &GgufConfig {
                model_dir,
                output_path: output_path.clone(),
                dtype,
            },
            &tx,
        )
        .expect("GGUF export should succeed");
        assert!(output_path.exists(), "GGUF file was not written");

        // 2. Load + run it through mistral.rs (the Onde SDK's engine).
        let reply = run_gguf_inference(&output_path, "Say hello in one short sentence.");
        eprintln!("exported GGUF reply: {reply:?}");
        assert!(
            !reply.trim().is_empty(),
            "exported GGUF produced an empty reply — model is not runnable"
        );
    }
}
