//! LoRA fine-tuning for Qwen-family models (Qwen2, Qwen2.5, Qwen3).
//!
//! Loads a safetensors base model from a HuggingFace cache directory, injects
//! trainable LoRA adapters into the Q and V projection layers of every
//! transformer block, trains for a configurable number of epochs on a JSONL
//! dataset, and saves the resulting adapter weights to a safetensors file.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use candle_core::{D, DType, Device, Tensor, Var};
use candle_nn::{AdamW, Optimizer, ParamsAdamW, VarBuilder};
use tokenizers::Tokenizer;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration for a LoRA fine-tuning run.
pub struct FineTuneConfig {
    /// Directory that contains `config.json`, `tokenizer.json`, and the
    /// safetensors weight files (single-file or sharded).
    pub model_dir: PathBuf,
    /// JSONL file where every line is `{"text": "…"}`.
    pub data_path: PathBuf,
    /// Directory where `lora_adapter.safetensors` will be written.
    pub output_dir: PathBuf,
    /// LoRA rank (default 8).
    pub lora_rank: usize,
    /// LoRA alpha, used to compute the scaling factor `alpha / rank` (default 16.0).
    pub lora_alpha: f32,
    /// Learning rate for AdamW (default 1e-4).
    pub learning_rate: f64,
    /// Number of full passes over the dataset (default 3).
    pub epochs: usize,
    /// Maximum number of tokens per training example (default 512).
    pub max_seq_len: usize,
}

/// Progress events emitted by the background fine-tuning thread.
pub enum FineTuneProgress {
    /// Checking that the model directory and data file exist.
    Validating,
    /// Loading and memory-mapping the model weights.
    LoadingModel,
    /// Tokenizing the dataset.
    Tokenizing { done: usize, total: usize },
    /// One optimizer step has completed.
    Training {
        epoch: usize,
        total_epochs: usize,
        step: usize,
        total_steps: usize,
        loss: f32,
    },
    /// Writing the adapter file to disk.
    Saving,
    /// Training finished successfully.
    Done { adapter_path: PathBuf },
    /// An unrecoverable error occurred.
    Failed(String),
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Spawn a background OS thread that runs the full fine-tuning pipeline and
/// streams [`FineTuneProgress`] events over `tx`.
///
/// The thread is intentionally a blocking `std::thread` (not a tokio task) so
/// that heavy tensor operations do not starve the async runtime.
pub fn start_finetune(
    config: FineTuneConfig,
    tx: tokio::sync::mpsc::UnboundedSender<FineTuneProgress>,
) {
    std::thread::spawn(move || {
        if let Err(e) = run_finetune(&config, &tx) {
            let _ = tx.send(FineTuneProgress::Failed(format!("{e:#}")));
        }
    });
}

// ---------------------------------------------------------------------------
// Private: top-level orchestration
// ---------------------------------------------------------------------------

fn run_finetune(
    config: &FineTuneConfig,
    tx: &tokio::sync::mpsc::UnboundedSender<FineTuneProgress>,
) -> Result<()> {
    // ------------------------------------------------------------------
    // Step A — Validate inputs
    // ------------------------------------------------------------------
    let _ = tx.send(FineTuneProgress::Validating);

    let model_dir = &config.model_dir;
    let config_json_path = model_dir.join("config.json");

    if !config_json_path.exists() {
        anyhow::bail!(
            "model config not found at {:?}; is this a valid HuggingFace model directory?",
            config_json_path
        );
    }
    if !config.data_path.exists() {
        anyhow::bail!("training data file not found at {:?}", config.data_path);
    }

    // ------------------------------------------------------------------
    // Step B — Load model config
    // ------------------------------------------------------------------
    let config_text = std::fs::read_to_string(&config_json_path)
        .with_context(|| format!("reading {:?}", config_json_path))?;
    let model_cfg: ModelConfig = serde_json::from_str(&config_text)
        .with_context(|| format!("parsing {:?}", config_json_path))?;

    // ------------------------------------------------------------------
    // Step C — Choose compute device
    // ------------------------------------------------------------------
    #[cfg(target_os = "macos")]
    let device = Device::new_metal(0).unwrap_or(Device::Cpu);
    #[cfg(not(target_os = "macos"))]
    let device = Device::Cpu;

    // ------------------------------------------------------------------
    // Step D — Load frozen base weights
    // ------------------------------------------------------------------
    let _ = tx.send(FineTuneProgress::LoadingModel);

    let index_path = model_dir.join("model.safetensors.index.json");
    let vb = if index_path.exists() {
        // Sharded model: read the weight-map JSON and collect unique shard paths.
        let index_text = std::fs::read_to_string(&index_path)
            .with_context(|| format!("reading {:?}", index_path))?;
        let index: IndexJson = serde_json::from_str(&index_text)
            .with_context(|| format!("parsing {:?}", index_path))?;

        let mut seen: HashSet<String> = HashSet::new();
        let mut shard_paths: Vec<PathBuf> = Vec::new();
        for filename in index.weight_map.values() {
            if seen.insert(filename.clone()) {
                shard_paths.push(model_dir.join(filename));
            }
        }
        shard_paths.sort();

        // SAFETY: the files are memory-mapped read-only.
        unsafe {
            VarBuilder::from_mmaped_safetensors(&shard_paths, DType::F32, &device)
                .context("loading sharded safetensors")?
        }
    } else {
        let single = model_dir.join("model.safetensors");
        // SAFETY: the file is memory-mapped read-only.
        unsafe {
            VarBuilder::from_mmaped_safetensors(&[single], DType::F32, &device)
                .context("loading model.safetensors")?
        }
    };

    // ------------------------------------------------------------------
    // Step E — Build LoRA model
    // ------------------------------------------------------------------
    let model = LoraQwenModel::load(vb, &model_cfg, config.lora_rank, config.lora_alpha, &device)
        .context("building LoRA model")?;

    // ------------------------------------------------------------------
    // Step F — Load and tokenize dataset
    // ------------------------------------------------------------------
    let tokenizer_path = model_dir.join("tokenizer.json");
    let tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow::anyhow!("loading tokenizer from {:?}: {e}", tokenizer_path))?;

    let file = std::fs::File::open(&config.data_path)
        .with_context(|| format!("opening {:?}", config.data_path))?;
    let reader = BufReader::new(file);
    let raw_lines: Vec<String> = reader
        .lines()
        .collect::<std::io::Result<Vec<_>>>()
        .context("reading training data")?;

    let total_lines = raw_lines.len();
    let mut token_batches: Vec<Vec<u32>> = Vec::new();

    for (i, line) in raw_lines.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            let _ = tx.send(FineTuneProgress::Tokenizing {
                done: i + 1,
                total: total_lines,
            });
            continue;
        }

        let entry: DataEntry =
            serde_json::from_str(line).with_context(|| format!("parsing JSONL line {}", i + 1))?;

        let encoding = tokenizer
            .encode(entry.text.as_str(), false)
            .map_err(|e| anyhow::anyhow!("tokenizing line {}: {e}", i + 1))?;

        let ids = encoding.get_ids();
        let ids: Vec<u32> = if ids.len() > config.max_seq_len {
            ids[..config.max_seq_len].to_vec()
        } else {
            ids.to_vec()
        };

        // Need at least 2 tokens for next-token prediction.
        if ids.len() >= 2 {
            token_batches.push(ids);
        }

        let _ = tx.send(FineTuneProgress::Tokenizing {
            done: i + 1,
            total: total_lines,
        });
    }

    if token_batches.is_empty() {
        anyhow::bail!("no valid training examples found in {:?}", config.data_path);
    }

    // ------------------------------------------------------------------
    // Step G — Training loop
    // ------------------------------------------------------------------
    let lora_vars = model.lora_vars();
    let adamw_params = ParamsAdamW {
        lr: config.learning_rate,
        ..ParamsAdamW::default()
    };
    let mut optimizer = AdamW::new(lora_vars, adamw_params).context("creating AdamW optimizer")?;

    let total_steps = token_batches.len();
    let vocab_size = model_cfg.vocab_size;

    for epoch in 0..config.epochs {
        for (step, batch_tokens) in token_batches.iter().enumerate() {
            let seq_len = batch_tokens.len();

            let input_ids = Tensor::from_slice(batch_tokens.as_slice(), (1, seq_len), &device)
                .context("building input_ids tensor")?;

            let logits = model.forward(&input_ids).context("model forward pass")?;

            // Next-token prediction: shift logits and targets by 1.
            let logits_shifted = logits
                .narrow(1, 0, seq_len - 1)
                .and_then(|t| t.reshape((seq_len - 1, vocab_size)))
                .context("preparing shifted logits")?;

            let targets = Tensor::from_slice(&batch_tokens[1..], (seq_len - 1,), &device)
                .context("building target tensor")?;

            let loss = candle_nn::loss::cross_entropy(&logits_shifted, &targets)
                .context("computing cross-entropy loss")?;

            optimizer.backward_step(&loss).context("optimizer step")?;

            let loss_val = loss.to_scalar::<f32>().context("reading loss scalar")?;

            let _ = tx.send(FineTuneProgress::Training {
                epoch: epoch + 1,
                total_epochs: config.epochs,
                step: step + 1,
                total_steps,
                loss: loss_val,
            });
        }
    }

    // ------------------------------------------------------------------
    // Step H — Save LoRA adapter
    // ------------------------------------------------------------------
    let _ = tx.send(FineTuneProgress::Saving);

    let adapter_path = model
        .save_lora(&config.output_dir)
        .context("saving LoRA adapter")?;

    let _ = tx.send(FineTuneProgress::Done { adapter_path });

    Ok(())
}

// ---------------------------------------------------------------------------
// Private: serde types for JSON deserialization
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ModelConfig {
    hidden_size: usize,
    num_hidden_layers: usize,
    num_attention_heads: usize,
    num_key_value_heads: Option<usize>,
    head_dim: Option<usize>,
    intermediate_size: usize,
    vocab_size: usize,
    rms_norm_eps: Option<f64>,
    rope_theta: Option<f64>,
    #[serde(default)]
    tie_word_embeddings: bool,
}

#[derive(serde::Deserialize)]
struct DataEntry {
    text: String,
}

/// Top-level structure of `model.safetensors.index.json`.
#[derive(serde::Deserialize)]
struct IndexJson {
    weight_map: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Private: tensor helpers
// ---------------------------------------------------------------------------

/// RMS layer normalisation: `x / rms(x) * weight`.
///
/// `weight` has shape `[hidden_size]`; `x` has shape `[batch, seq, hidden_size]`.
fn rms_norm(x: &Tensor, weight: &Tensor, eps: f64) -> candle_core::Result<Tensor> {
    // mean of squares across the last (hidden) dimension → [batch, seq, 1]
    let mean_sq = x.sqr()?.mean_keepdim(D::Minus1)?;
    // x / sqrt(mean_sq + eps)
    let normed = x.broadcast_div(&mean_sq.affine(1.0, eps)?.sqrt()?)?;
    // scale by the learned weight
    normed.broadcast_mul(weight)
}

/// Apply rotary position embeddings to `x`.
///
/// `x`   has shape `[batch, heads, seq, head_dim]`.
/// `cos` has shape `[seq, head_dim]` (already narrowed to the current sequence length).
/// `sin` has shape `[seq, head_dim]`.
fn apply_rope(x: &Tensor, cos: &Tensor, sin: &Tensor) -> candle_core::Result<Tensor> {
    let head_dim = x.dim(D::Minus1)?;
    let half = head_dim / 2;

    let x1 = x.narrow(D::Minus1, 0, half)?; // first half
    let x2 = x.narrow(D::Minus1, half, half)?; // second half

    // rotate_half: [-x2, x1]
    let neg_x2 = x2.neg()?;
    let rotated = Tensor::cat(&[&neg_x2, &x1], D::Minus1)?;

    // Unsqueeze cos/sin to [1, 1, seq, head_dim] for broadcasting.
    let cos = cos.unsqueeze(0)?.unsqueeze(0)?;
    let sin = sin.unsqueeze(0)?.unsqueeze(0)?;

    x.broadcast_mul(&cos)? + rotated.broadcast_mul(&sin)?
}

/// Precompute cosine and sine tables for RoPE.
///
/// Returns `(cos, sin)` each with shape `[max_seq_len, head_dim]`.
fn precompute_rope(
    head_dim: usize,
    max_seq_len: usize,
    theta: f64,
    device: &Device,
) -> candle_core::Result<(Tensor, Tensor)> {
    let half = head_dim / 2;

    // Inverse frequencies: 1 / theta^(2i / head_dim) for i in 0..half
    let freqs: Vec<f32> = (0..half)
        .map(|i| 1.0f32 / (theta as f32).powf(2.0 * i as f32 / head_dim as f32))
        .collect();

    // freqs: [1, half]
    let freqs = Tensor::from_slice(freqs.as_slice(), (1, half), device)?;

    // position indices: [max_seq_len, 1]
    let pos: Vec<f32> = (0..max_seq_len).map(|i| i as f32).collect();
    let pos = Tensor::from_slice(pos.as_slice(), (max_seq_len, 1), device)?;

    // outer product → [max_seq_len, half]
    let freqs = pos.broadcast_mul(&freqs)?;

    // Duplicate halves so both halves receive the same rotation angle.
    // Result shape: [max_seq_len, head_dim]
    let cos = Tensor::cat(&[&freqs.cos()?, &freqs.cos()?], 1)?;
    let sin = Tensor::cat(&[&freqs.sin()?, &freqs.sin()?], 1)?;

    Ok((cos, sin))
}

// ---------------------------------------------------------------------------
// Private: LoRA-augmented linear layer
// ---------------------------------------------------------------------------

struct LoraLinear {
    /// Frozen base weight `[out_features, in_features]`.
    weight: Tensor,
    /// Optional frozen base bias `[out_features]`.
    bias: Option<Tensor>,
    /// Trainable LoRA A matrix `[rank, in_features]`, initialised N(0, 1/√rank).
    lora_a: Var,
    /// Trainable LoRA B matrix `[out_features, rank]`, initialised to zeros.
    lora_b: Var,
    /// Scaling factor `alpha / rank`.
    scale: f64,
}

impl LoraLinear {
    fn new(
        weight: Tensor,
        bias: Option<Tensor>,
        rank: usize,
        alpha: f32,
        device: &Device,
    ) -> candle_core::Result<Self> {
        let shape = weight.shape().dims();
        // weight is [out_features, in_features]
        let out_features = shape[0];
        let in_features = shape[1];

        let std_val = (1.0 / (rank as f64).sqrt()) as f32;
        let lora_a = Var::randn(0.0f32, std_val, (rank, in_features), device)?;
        let lora_b = Var::zeros((out_features, rank), DType::F32, device)?;
        let scale = alpha as f64 / rank as f64;

        Ok(Self {
            weight,
            bias,
            lora_a,
            lora_b,
            scale,
        })
    }

    /// `y = x W^T + b  +  (x A^T) B^T * scale`
    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        // Base projection
        let base = x.matmul(&self.weight.t()?)?;
        let base = match &self.bias {
            Some(b) => base.broadcast_add(b)?,
            None => base,
        };

        // LoRA path: x A^T → x B^T scaled
        let lora = x
            .matmul(&self.lora_a.as_tensor().t()?)?
            .matmul(&self.lora_b.as_tensor().t()?)?
            .affine(self.scale, 0.0)?;

        base + lora
    }

    /// Return the two trainable LoRA variables.
    fn vars(&self) -> Vec<Var> {
        vec![self.lora_a.clone(), self.lora_b.clone()]
    }
}

// ---------------------------------------------------------------------------
// Private: frozen linear layer (no LoRA)
// ---------------------------------------------------------------------------

struct FrozenLinear {
    weight: Tensor,
    bias: Option<Tensor>,
}

impl FrozenLinear {
    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        let out = x.matmul(&self.weight.t()?)?;
        match &self.bias {
            Some(b) => out.broadcast_add(b),
            None => Ok(out),
        }
    }
}

// ---------------------------------------------------------------------------
// Private: single transformer block
// ---------------------------------------------------------------------------

struct TransformerLayer {
    // Attention projections
    q_proj: LoraLinear,   // LoRA applied
    k_proj: FrozenLinear, // frozen
    v_proj: LoraLinear,   // LoRA applied
    o_proj: FrozenLinear, // frozen
    // QK-norm (Qwen3 only — None for Qwen2/2.5)
    q_norm: Option<Tensor>, // [head_dim]
    k_norm: Option<Tensor>, // [head_dim]
    // MLP projections (SwiGLU)
    gate_proj: FrozenLinear,
    up_proj: FrozenLinear,
    down_proj: FrozenLinear,
    // LayerNorm weights (RMSNorm, no bias)
    input_layernorm: Tensor,          // [hidden_size]
    post_attention_layernorm: Tensor, // [hidden_size]
    // Hyperparameters
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
    rms_norm_eps: f64,
}

impl TransformerLayer {
    /// Full pre-norm transformer block forward pass.
    ///
    /// `x`   has shape `[batch, seq, hidden_size]`.
    /// `cos` / `sin` have shape `[seq, head_dim]` (sliced from the precomputed tables).
    fn forward(&self, x: &Tensor, cos: &Tensor, sin: &Tensor) -> candle_core::Result<Tensor> {
        let (b, seq, _hidden) = x.dims3()?;

        // ----- Attention sub-layer -----

        // Pre-attention RMSNorm
        let normed = rms_norm(x, &self.input_layernorm, self.rms_norm_eps)?;

        // QKV projections
        let q = self.q_proj.forward(&normed)?; // [b, seq, num_heads * head_dim]
        let k = self.k_proj.forward(&normed)?; // [b, seq, num_kv_heads * head_dim]
        let v = self.v_proj.forward(&normed)?; // [b, seq, num_kv_heads * head_dim]

        // Reshape to [b, heads, seq, head_dim]
        let q = q
            .reshape((b, seq, self.num_heads, self.head_dim))?
            .transpose(1, 2)?;
        let k = k
            .reshape((b, seq, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;
        let v = v
            .reshape((b, seq, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;

        // Qwen3 QK-Norm: apply per-head RMSNorm to Q and K before RoPE.
        let q = if let Some(w) = &self.q_norm {
            rms_norm(&q, w, self.rms_norm_eps)?
        } else {
            q
        };
        let k = if let Some(w) = &self.k_norm {
            rms_norm(&k, w, self.rms_norm_eps)?
        } else {
            k
        };

        // Apply RoPE (requires contiguous storage for the custom kernel)
        let q = apply_rope(&q.contiguous()?, cos, sin)?;
        let k = apply_rope(&k.contiguous()?, cos, sin)?;

        // Grouped Query Attention: expand K/V heads if num_heads > num_kv_heads
        let groups = self.num_heads / self.num_kv_heads;
        let (k, v) = if groups == 1 {
            (k.contiguous()?, v.contiguous()?)
        } else {
            let k_exp = k
                .unsqueeze(2)?
                .expand((b, self.num_kv_heads, groups, seq, self.head_dim))?
                .reshape((b, self.num_heads, seq, self.head_dim))?
                .contiguous()?;
            let v_exp = v
                .unsqueeze(2)?
                .expand((b, self.num_kv_heads, groups, seq, self.head_dim))?
                .reshape((b, self.num_heads, seq, self.head_dim))?
                .contiguous()?;
            (k_exp, v_exp)
        };

        // Scaled dot-product attention scores
        let scale = 1.0 / (self.head_dim as f64).sqrt();
        let attn_weights = (q.matmul(&k.transpose(2, 3)?)? * scale)?;

        // Causal mask: add -1e9 to positions the query must not attend to.
        let device = x.device();
        let tril =
            Tensor::tril2(seq, DType::F32, device)?.broadcast_as((b, self.num_heads, seq, seq))?;
        // (1 - tril) is 1 on upper triangle (blocked), 0 on lower triangle (allowed).
        let attn_bias = (1.0f64 - &tril)?.affine(-1e9, 0.0)?;
        let attn_weights = (attn_weights + attn_bias)?;
        let attn_weights = candle_nn::ops::softmax_last_dim(&attn_weights)?;

        // Weighted sum of values
        let attn_out = attn_weights.matmul(&v)?;

        // Merge heads: [b, heads, seq, head_dim] → [b, seq, hidden_size]
        let attn_out = attn_out.transpose(1, 2)?.contiguous()?.reshape((
            b,
            seq,
            self.num_heads * self.head_dim,
        ))?;

        // Output projection
        let attn_out = self.o_proj.forward(&attn_out)?;

        // First residual connection
        let h = (x + &attn_out)?;

        // ----- MLP sub-layer (SwiGLU) -----

        // Pre-MLP RMSNorm
        let normed2 = rms_norm(&h, &self.post_attention_layernorm, self.rms_norm_eps)?;

        let gate = candle_nn::ops::silu(&self.gate_proj.forward(&normed2)?)?;
        let up = self.up_proj.forward(&normed2)?;
        let mlp_out = self.down_proj.forward(&(gate * up)?)?;

        // Second residual connection
        h + mlp_out
    }

    /// All trainable LoRA variables in this layer.
    fn lora_vars(&self) -> Vec<Var> {
        let mut v = self.q_proj.vars();
        v.extend(self.v_proj.vars());
        v
    }
}

// ---------------------------------------------------------------------------
// Private: full model
// ---------------------------------------------------------------------------

struct LoraQwenModel {
    embed_tokens: Tensor, // [vocab_size, hidden_size]
    layers: Vec<TransformerLayer>,
    norm: Tensor,     // [hidden_size] — final RMSNorm weight
    lm_head: Tensor,  // [vocab_size, hidden_size]
    rope_cos: Tensor, // [max_seq_len, head_dim]
    rope_sin: Tensor, // [max_seq_len, head_dim]
    hidden_size: usize,
    #[allow(dead_code)]
    vocab_size: usize,
    rms_norm_eps: f64,
}

impl LoraQwenModel {
    /// Load base weights from a `VarBuilder` and inject LoRA adapters.
    fn load(
        vb: VarBuilder,
        cfg: &ModelConfig,
        rank: usize,
        alpha: f32,
        device: &Device,
    ) -> candle_core::Result<Self> {
        let hidden_size = cfg.hidden_size;
        let vocab_size = cfg.vocab_size;
        let num_heads = cfg.num_attention_heads;
        let num_kv_heads = cfg.num_key_value_heads.unwrap_or(num_heads);
        let head_dim = cfg.head_dim.unwrap_or(hidden_size / num_heads);
        let intermediate_size = cfg.intermediate_size;
        let rms_norm_eps = cfg.rms_norm_eps.unwrap_or(1e-6);
        let rope_theta = cfg.rope_theta.unwrap_or(10_000.0);

        // Maximum sequence length for the RoPE tables.
        // 2048 covers all models targeted by this module; increase if needed.
        let max_seq_len = 2048usize;

        // Embedding table
        let embed_tokens = vb
            .pp("model")
            .pp("embed_tokens")
            .get((vocab_size, hidden_size), "weight")?;

        // Transformer layers
        let mut layers: Vec<TransformerLayer> = Vec::with_capacity(cfg.num_hidden_layers);

        for i in 0..cfg.num_hidden_layers {
            let vb_layer = vb.pp("model").pp("layers").pp(i.to_string());

            let vb_attn = vb_layer.pp("self_attn");

            // --- Q projection (LoRA) ---
            let q_out = num_heads * head_dim;
            let q_weight = vb_attn.pp("q_proj").get((q_out, hidden_size), "weight")?;
            // Qwen2 has bias on q/k/v; Qwen3 does not — use .ok() to handle both.
            let q_bias = vb_attn.pp("q_proj").get((q_out,), "bias").ok();
            let q_proj = LoraLinear::new(q_weight, q_bias, rank, alpha, device)?;

            // --- K projection (frozen) ---
            let kv_out = num_kv_heads * head_dim;
            let k_weight = vb_attn.pp("k_proj").get((kv_out, hidden_size), "weight")?;
            let k_bias = vb_attn.pp("k_proj").get((kv_out,), "bias").ok();
            let k_proj = FrozenLinear {
                weight: k_weight,
                bias: k_bias,
            };

            // --- V projection (LoRA) ---
            let v_weight = vb_attn.pp("v_proj").get((kv_out, hidden_size), "weight")?;
            let v_bias = vb_attn.pp("v_proj").get((kv_out,), "bias").ok();
            let v_proj = LoraLinear::new(v_weight, v_bias, rank, alpha, device)?;

            // --- O projection (frozen) ---
            let o_weight = vb_attn
                .pp("o_proj")
                .get((hidden_size, num_heads * head_dim), "weight")?;
            let o_proj = FrozenLinear {
                weight: o_weight,
                bias: None,
            };

            // --- QK-Norm (Qwen3 only) ---
            let q_norm = vb_attn.pp("q_norm").get((head_dim,), "weight").ok();
            let k_norm = vb_attn.pp("k_norm").get((head_dim,), "weight").ok();

            // --- MLP projections (all frozen) ---
            let vb_mlp = vb_layer.pp("mlp");

            let gate_weight = vb_mlp
                .pp("gate_proj")
                .get((intermediate_size, hidden_size), "weight")?;
            let up_weight = vb_mlp
                .pp("up_proj")
                .get((intermediate_size, hidden_size), "weight")?;
            let down_weight = vb_mlp
                .pp("down_proj")
                .get((hidden_size, intermediate_size), "weight")?;

            // --- LayerNorm weights ---
            let input_layernorm = vb_layer
                .pp("input_layernorm")
                .get((hidden_size,), "weight")?;
            let post_attention_layernorm = vb_layer
                .pp("post_attention_layernorm")
                .get((hidden_size,), "weight")?;

            layers.push(TransformerLayer {
                q_proj,
                k_proj,
                v_proj,
                o_proj,
                q_norm,
                k_norm,
                gate_proj: FrozenLinear {
                    weight: gate_weight,
                    bias: None,
                },
                up_proj: FrozenLinear {
                    weight: up_weight,
                    bias: None,
                },
                down_proj: FrozenLinear {
                    weight: down_weight,
                    bias: None,
                },
                input_layernorm,
                post_attention_layernorm,
                num_heads,
                num_kv_heads,
                head_dim,
                rms_norm_eps,
            });
        }

        // Final RMSNorm and language model head
        let norm = vb.pp("model").pp("norm").get((hidden_size,), "weight")?;
        let lm_head = vb
            .pp("lm_head")
            .get((vocab_size, hidden_size), "weight")
            .unwrap_or_else(|_| embed_tokens.clone());

        // Precompute RoPE tables
        let (rope_cos, rope_sin) = precompute_rope(head_dim, max_seq_len, rope_theta, device)?;

        Ok(Self {
            embed_tokens,
            layers,
            norm,
            lm_head,
            rope_cos,
            rope_sin,
            hidden_size,
            vocab_size,
            rms_norm_eps,
        })
    }

    /// Full forward pass.
    ///
    /// `input_ids` has shape `[batch, seq]`.
    /// Returns logits with shape `[batch, seq, vocab_size]`.
    fn forward(&self, input_ids: &Tensor) -> candle_core::Result<Tensor> {
        let (b, seq) = input_ids.dims2()?;

        // Token embedding lookup: [batch, seq, hidden_size]
        let ids_flat = input_ids.flatten_all()?; // [b * seq]
        let mut hidden = self
            .embed_tokens
            .embedding(&ids_flat)? // [b * seq, hidden_size]
            .reshape((b, seq, self.hidden_size))?; // [b, seq, hidden_size]

        // Slice RoPE tables to the current sequence length.
        let cos = self.rope_cos.narrow(0, 0, seq)?; // [seq, head_dim]
        let sin = self.rope_sin.narrow(0, 0, seq)?; // [seq, head_dim]

        // Transformer layers
        for layer in &self.layers {
            hidden = layer.forward(&hidden, &cos, &sin)?;
        }

        // Final RMSNorm
        let hidden = rms_norm(&hidden, &self.norm, self.rms_norm_eps)?;

        // Language model head: [b, seq, vocab_size]
        hidden.matmul(&self.lm_head.t()?)
    }

    /// Collect all trainable LoRA variables across every layer.
    fn lora_vars(&self) -> Vec<Var> {
        self.layers
            .iter()
            .flat_map(|layer| layer.lora_vars())
            .collect()
    }

    /// Serialise the LoRA adapter weights to `output_dir/lora_adapter.safetensors`.
    ///
    /// Saves one `lora_a` and one `lora_b` tensor per projection (Q and V) per layer.
    fn save_lora(&self, output_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("creating output directory {:?}", output_dir))?;

        let mut tensors: HashMap<String, Tensor> = HashMap::new();

        for (i, layer) in self.layers.iter().enumerate() {
            tensors.insert(
                format!("model.layers.{i}.self_attn.q_proj.lora_a"),
                layer.q_proj.lora_a.as_tensor().clone(),
            );
            tensors.insert(
                format!("model.layers.{i}.self_attn.q_proj.lora_b"),
                layer.q_proj.lora_b.as_tensor().clone(),
            );
            tensors.insert(
                format!("model.layers.{i}.self_attn.v_proj.lora_a"),
                layer.v_proj.lora_a.as_tensor().clone(),
            );
            tensors.insert(
                format!("model.layers.{i}.self_attn.v_proj.lora_b"),
                layer.v_proj.lora_b.as_tensor().clone(),
            );
        }

        let path = output_dir.join("lora_adapter.safetensors");
        candle_core::safetensors::save(&tensors, &path)
            .with_context(|| format!("writing {:?}", path))?;

        Ok(path)
    }
}
