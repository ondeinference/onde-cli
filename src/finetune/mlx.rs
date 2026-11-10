//! MLX LoRA trainer for Apple Silicon.
//!
//! Same model, loss, gradient guard and optimiser as the candle trainer in the
//! parent module, run on MLX. The adapter is written through candle's
//! safetensors writer under the same keys, so `merge.rs` needs no changes.
//!
//! Semantics mirrored from candle:
//! - base weights in f32; q/k/v biases when the checkpoint has them (Qwen2/2.5);
//!   `q_norm`/`k_norm` only when present (Qwen3);
//! - LoRA on q and v: A `[r, in]` ~ N(0, 1/sqrt(r)), B `[out, r]` = 0, scale alpha/r;
//! - loss = mean NLL over positions where `mask[1..]` is true;
//! - non-finite gradient elements are zeroed, the global L2 norm is computed in a
//!   scaled space, a zero or non-finite norm skips the step loudly, and the
//!   gradient is clipped to norm 1.0;
//! - AdamW with candle's defaults (0.9, 0.999, 1e-8, decoupled decay 0.01) and its
//!   exact update order.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use anyhow::{Context, Result};
use candle_core::{Device, Tensor};
use mlx_rs::{
    Array,
    error::Exception,
    fast::{self, ScaledDotProductAttentionMask},
    nn::log_softmax,
    ops,
    transforms::{eval, keyed_value_and_grad},
};

use super::{FineTuneConfig, FineTuneProgress, IndexJson, ModelConfig, TrainingExample};

type Params = HashMap<Rc<str>, Array>;
type Mlx<T> = std::result::Result<T, Exception>;

/// Fixed seed so a run is reproducible.
const SEED: u64 = 0x00DE_5EED;
const CACHE_LIMIT_BYTES: usize = 1 << 30;
const MAX_GRAD_NORM: f64 = 1.0;
/// candle-nn `ParamsAdamW::default()` apart from the learning rate.
const BETA1: f64 = 0.9;
const BETA2: f64 = 0.999;
const ADAM_EPS: f64 = 1e-8;
const WEIGHT_DECAY: f64 = 0.01;

fn scalar(x: f64) -> Array {
    Array::from_f32(x as f32)
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Dims {
    heads: i32,
    kv_heads: i32,
    head_dim: i32,
    layers: usize,
    eps: f32,
    theta: f32,
    tie_embeddings: bool,
}

struct MlxModel {
    dims: Dims,
    weights: HashMap<String, Array>,
    /// LoRA scaling, alpha / rank.
    lora_scale: f32,
}

/// One example prepared for the loss: token ids, the positions that carry loss
/// and their targets.
struct Batch {
    ids: Array,
    keep: Array,
    targets: Array,
    seq_len: usize,
}

impl Batch {
    /// `None` when no position carries loss. `mask[1..]` selects which next-token
    /// predictions count, exactly as in the candle trainer.
    fn new(tokens: &[u32], mask: &[bool]) -> Option<Self> {
        let n = tokens.len();
        let keep: Vec<i32> = (0..n - 1)
            .filter(|&j| mask[j + 1])
            .map(|j| j as i32)
            .collect();
        if keep.is_empty() {
            return None;
        }
        let targets: Vec<i32> = keep
            .iter()
            .map(|&j| tokens[j as usize + 1] as i32)
            .collect();
        let ids: Vec<i32> = tokens.iter().map(|&t| t as i32).collect();
        let k = keep.len() as i32;
        Some(Self {
            ids: Array::from_slice(&ids, &[n as i32]),
            keep: Array::from_slice(&keep, &[k]),
            targets: Array::from_slice(&targets, &[k, 1]),
            seq_len: n,
        })
    }
}

/// Keys of the tensors the forward pass reads. Optional ones are listed
/// separately and only loaded when the checkpoint has them.
fn required_keys(cfg: &ModelConfig, layers: usize) -> (Vec<String>, Vec<String>) {
    let mut required = vec![
        "model.embed_tokens.weight".to_string(),
        "model.norm.weight".to_string(),
    ];
    let mut optional = Vec::new();
    if !cfg.tie_word_embeddings {
        required.push("lm_head.weight".into());
    }
    for i in 0..layers {
        let p = format!("model.layers.{i}");
        for s in [
            "self_attn.q_proj.weight",
            "self_attn.k_proj.weight",
            "self_attn.v_proj.weight",
            "self_attn.o_proj.weight",
            "mlp.gate_proj.weight",
            "mlp.up_proj.weight",
            "mlp.down_proj.weight",
            "input_layernorm.weight",
            "post_attention_layernorm.weight",
        ] {
            required.push(format!("{p}.{s}"));
        }
        for s in [
            "self_attn.q_proj.bias",
            "self_attn.k_proj.bias",
            "self_attn.v_proj.bias",
            "self_attn.q_norm.weight",
            "self_attn.k_norm.weight",
        ] {
            optional.push(format!("{p}.{s}"));
        }
    }
    (required, optional)
}

/// Safetensors files of a model directory: the shards listed in
/// `model.safetensors.index.json`, or the single `model.safetensors`.
fn weight_files(model_dir: &Path) -> Result<Vec<PathBuf>> {
    let index_path = model_dir.join("model.safetensors.index.json");
    if index_path.exists() {
        let text = std::fs::read_to_string(&index_path)
            .with_context(|| format!("reading {index_path:?}"))?;
        let index: IndexJson =
            serde_json::from_str(&text).with_context(|| format!("parsing {index_path:?}"))?;
        let mut seen = HashSet::new();
        let mut paths: Vec<PathBuf> = index
            .weight_map
            .values()
            .filter(|f| seen.insert((*f).clone()))
            .map(|f| model_dir.join(f))
            .collect();
        paths.sort();
        Ok(paths)
    } else {
        Ok(vec![model_dir.join("model.safetensors")])
    }
}

impl MlxModel {
    fn load(model_dir: &Path, cfg: &ModelConfig, rank: usize, alpha: f32) -> Result<Self> {
        let mut all: HashMap<String, Array> = HashMap::new();
        for path in weight_files(model_dir)? {
            let part = Array::load_safetensors(&path)
                .map_err(|e| anyhow::anyhow!("loading {path:?}: {e}"))?;
            all.extend(part);
        }
        Self::from_weights(cfg, all, rank, alpha)
    }

    /// Pick the tensors the forward pass uses out of `all` (dropping the rest),
    /// cast them to f32 and materialise them once.
    fn from_weights(
        cfg: &ModelConfig,
        mut all: HashMap<String, Array>,
        rank: usize,
        alpha: f32,
    ) -> Result<Self> {
        let layers = cfg.num_hidden_layers;
        let (required, optional) = required_keys(cfg, layers);
        let mut weights = HashMap::new();
        for key in required {
            let w = all
                .remove(&key)
                .with_context(|| format!("missing tensor {key} in the checkpoint"))?;
            weights.insert(key, w.as_type::<f32>().map_err(|e| anyhow::anyhow!("{e}"))?);
        }
        for key in optional {
            if let Some(w) = all.remove(&key) {
                weights.insert(key, w.as_type::<f32>().map_err(|e| anyhow::anyhow!("{e}"))?);
            }
        }
        drop(all);
        eval(weights.values()).map_err(|e| anyhow::anyhow!("materialising weights: {e}"))?;

        Ok(Self {
            dims: Dims {
                heads: cfg.num_attention_heads as i32,
                kv_heads: cfg.num_kv_heads() as i32,
                head_dim: cfg.head_dim() as i32,
                layers,
                eps: cfg.rms_norm_eps() as f32,
                theta: cfg.rope_theta() as f32,
                tie_embeddings: cfg.tie_word_embeddings,
            },
            weights,
            lora_scale: alpha / rank as f32,
        })
    }

    fn w(&self, key: &str) -> &Array {
        &self.weights[key]
    }

    /// `x W^T (+ b)`.
    fn frozen(&self, x: &Array, prefix: &str) -> Mlx<Array> {
        let y = x.matmul(self.w(&format!("{prefix}.weight")).t())?;
        match self.weights.get(&format!("{prefix}.bias")) {
            Some(b) => y.add(b),
            None => Ok(y),
        }
    }

    /// `x W^T (+ b) + (x A^T) B^T * scale`.
    fn lora_linear(&self, x: &Array, prefix: &str, a: &Array, b: &Array) -> Mlx<Array> {
        let base = self.frozen(x, prefix)?;
        let lora = x
            .matmul(a.t())?
            .matmul(b.t())?
            .multiply(scalar(self.lora_scale as f64))?;
        base.add(lora)
    }

    /// Final-norm hidden states `[n, hidden]` for token ids `[n]`.
    fn hidden(&self, lora: &Params, ids: &Array) -> Mlx<Array> {
        let d = &self.dims;
        let n = ids.shape()[0];
        let mut h = self.w("model.embed_tokens.weight").take_axis(ids, 0)?;
        let split = |t: &Array, heads: i32| -> Mlx<Array> {
            t.reshape(&[1, n, heads, d.head_dim])?
                .transpose_axes(&[0, 2, 1, 3])
        };
        for i in 0..d.layers {
            let p = format!("model.layers.{i}");
            let attn = format!("{p}.self_attn");
            let l = |s: &str| &lora[&*format!("{attn}.{s}")];

            let normed = fast::rms_norm(
                &h,
                Some(self.w(&format!("{p}.input_layernorm.weight"))),
                d.eps,
            )?;
            let q = self.lora_linear(
                &normed,
                &format!("{attn}.q_proj"),
                l("q_proj.lora_a"),
                l("q_proj.lora_b"),
            )?;
            let k = self.frozen(&normed, &format!("{attn}.k_proj"))?;
            let v = self.lora_linear(
                &normed,
                &format!("{attn}.v_proj"),
                l("v_proj.lora_a"),
                l("v_proj.lora_b"),
            )?;
            let (mut q, mut k, v) = (
                split(&q, d.heads)?,
                split(&k, d.kv_heads)?,
                split(&v, d.kv_heads)?,
            );

            // Qwen3 QK-norm, per head, before RoPE.
            if let Some(w) = self.weights.get(&format!("{attn}.q_norm.weight")) {
                q = fast::rms_norm(&q, Some(w), d.eps)?;
            }
            if let Some(w) = self.weights.get(&format!("{attn}.k_norm.weight")) {
                k = fast::rms_norm(&k, Some(w), d.eps)?;
            }
            let q = fast::rope(&q, d.head_dim, false, Some(d.theta), 1.0, 0, None)?;
            let k = fast::rope(&k, d.head_dim, false, Some(d.theta), 1.0, 0, None)?;

            // GQA is handled inside SDPA (fewer K/V heads than Q heads).
            let scale = 1.0 / (d.head_dim as f32).sqrt();
            let o = fast::scaled_dot_product_attention(
                &q,
                &k,
                &v,
                scale,
                ScaledDotProductAttentionMask::Causal,
                None,
            )?;
            let o = o
                .transpose_axes(&[0, 2, 1, 3])?
                .reshape(&[n, d.heads * d.head_dim])?;
            let o = self.frozen(&o, &format!("{attn}.o_proj"))?;
            h = h.add(o)?;

            let normed2 = fast::rms_norm(
                &h,
                Some(self.w(&format!("{p}.post_attention_layernorm.weight"))),
                d.eps,
            )?;
            let gate = mlx_rs::nn::silu(self.frozen(&normed2, &format!("{p}.mlp.gate_proj"))?)?;
            let up = self.frozen(&normed2, &format!("{p}.mlp.up_proj"))?;
            let down = self.frozen(&gate.multiply(up)?, &format!("{p}.mlp.down_proj"))?;
            h = h.add(down)?;
        }
        fast::rms_norm(&h, Some(self.w("model.norm.weight")), d.eps)
    }

    fn lm_head(&self) -> &Array {
        if self.dims.tie_embeddings {
            self.w("model.embed_tokens.weight")
        } else {
            self.w("lm_head.weight")
        }
    }

    /// Mean next-token NLL over the kept positions. Only those rows go through
    /// the (vocab-sized) head, which is the same set candle's `index_select`
    /// keeps.
    fn loss(&self, lora: &Params, batch: &Batch) -> Mlx<Array> {
        let h = self.hidden(lora, &batch.ids)?;
        let rows = h.take_axis(&batch.keep, 0)?;
        let logits = rows.matmul(self.lm_head().t())?;
        let lp = log_softmax(&logits, -1)?;
        lp.take_along_axis(&batch.targets, -1)?
            .mean(None)?
            .negative()
    }

    fn loss_and_grads(&self, lora: &Params, batch: &Batch) -> Mlx<(Array, Params)> {
        let f = |p: Params, _: i32| -> Mlx<Vec<Array>> { Ok(vec![self.loss(&p, batch)?]) };
        let mut vg = keyed_value_and_grad(f);
        let (v, g) = vg(lora.clone(), 0)?;
        Ok((v.into_iter().next().expect("one loss value"), g))
    }

    /// A ~ N(0, 1/sqrt(rank)), B = 0 for q and v of every layer.
    fn init_lora(&self, rank: usize) -> Result<Params> {
        let d = &self.dims;
        let mut lora = Params::new();
        let rank_i = rank as i32;
        let std = 1.0 / (rank as f32).sqrt();
        for i in 0..d.layers {
            for name in ["q_proj", "v_proj"] {
                let w = self.w(&format!("model.layers.{i}.self_attn.{name}.weight"));
                let (out, inp) = (w.shape()[0], w.shape()[1]);
                let a = mlx_rs::random::normal::<f32>(&[rank_i, inp], 0.0, std, None)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let b = ops::zeros::<f32>(&[out, rank_i]).map_err(|e| anyhow::anyhow!("{e}"))?;
                lora.insert(
                    format!("model.layers.{i}.self_attn.{name}.lora_a").into(),
                    a,
                );
                lora.insert(
                    format!("model.layers.{i}.self_attn.{name}.lora_b").into(),
                    b,
                );
            }
        }
        eval(lora.values()).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(lora)
    }
}

// ---------------------------------------------------------------------------
// Gradient guard and optimiser
// ---------------------------------------------------------------------------

/// Replace non-finite elements (NaN, +-Inf, |x| > 1e30) with zero, as
/// `sanitize_grad` does for candle.
fn sanitize(grad: &Array) -> Mlx<Array> {
    let finite = grad.abs()?.le(scalar(1e30))?;
    ops::select(&finite, grad, ops::zeros_like(grad)?)
}

/// `(max |g|, L2 norm)` over all gradients, the norm computed on `g / max` so
/// the f32 sum cannot overflow. Same two-pass scheme as the candle trainer.
fn grad_stats(clean: &Params) -> Result<(f64, f64)> {
    let maxes = clean
        .values()
        .map(|g| g.abs()?.max(None))
        .collect::<Mlx<Vec<_>>>()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    eval(maxes.iter()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let max_abs = maxes
        .iter()
        .map(|m| m.item_exact::<f32>() as f64)
        .fold(0.0, f64::max);
    if max_abs <= 0.0 {
        return Ok((max_abs, 0.0));
    }
    let inv = scalar(1.0 / max_abs);
    let sums = clean
        .values()
        .map(|g| {
            let s = g.multiply(&inv)?;
            s.multiply(&s)?.sum(None)
        })
        .collect::<Mlx<Vec<_>>>()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    eval(sums.iter()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let sq: f64 = sums.iter().map(|s| s.item_exact::<f32>() as f64).sum();
    Ok((max_abs, max_abs * sq.sqrt()))
}

/// Scale every gradient so the global norm is at most [`MAX_GRAD_NORM`].
fn clip(grads: &mut Params, total_norm: f64) -> Mlx<()> {
    if total_norm > MAX_GRAD_NORM {
        let coef = scalar(MAX_GRAD_NORM / (total_norm + 1e-6));
        for g in grads.values_mut() {
            *g = g.multiply(&coef)?;
        }
    }
    Ok(())
}

/// candle-nn's `AdamW::step`, in the same operation order.
struct AdamW {
    lr: f64,
    step_t: i32,
    m: Params,
    v: Params,
}

impl AdamW {
    fn new(params: &Params, lr: f64) -> Mlx<Self> {
        let zeros = |p: &Params| -> Mlx<Params> {
            p.iter()
                .map(|(k, a)| Ok((k.clone(), ops::zeros_like(a)?)))
                .collect()
        };
        Ok(Self {
            lr,
            step_t: 0,
            m: zeros(params)?,
            v: zeros(params)?,
        })
    }

    fn step(&mut self, params: &mut Params, grads: &Params) -> Mlx<()> {
        self.step_t += 1;
        let lr_lambda = self.lr * WEIGHT_DECAY;
        let scale_m = scalar(1.0 / (1.0 - BETA1.powi(self.step_t)));
        let scale_v = scalar(1.0 / (1.0 - BETA2.powi(self.step_t)));
        for (k, g) in grads {
            let theta = &params[k];
            let next_m = self.m[k]
                .multiply(scalar(BETA1))?
                .add(g.multiply(scalar(1.0 - BETA1))?)?;
            let next_v = self.v[k]
                .multiply(scalar(BETA2))?
                .add(g.square()?.multiply(scalar(1.0 - BETA2))?)?;
            let m_hat = next_m.multiply(&scale_m)?;
            let v_hat = next_v.multiply(&scale_v)?;
            let next_theta = theta.multiply(scalar(1.0 - lr_lambda))?;
            let adjusted = m_hat.divide(v_hat.sqrt()?.add(scalar(ADAM_EPS))?)?;
            let next_theta = next_theta.subtract(adjusted.multiply(scalar(self.lr))?)?;
            self.m.insert(k.clone(), next_m);
            self.v.insert(k.clone(), next_v);
            params.insert(k.clone(), next_theta);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Training loop
// ---------------------------------------------------------------------------

pub(super) fn train_mlx(
    config: &FineTuneConfig,
    model_cfg: &ModelConfig,
    examples: &[TrainingExample],
    tx: &tokio::sync::mpsc::UnboundedSender<FineTuneProgress>,
) -> Result<()> {
    let _ = tx.send(FineTuneProgress::LoadingModel);
    // Without a cap MLX keeps every freed buffer cached: a 1.5B run peaked at
    // 21.9 GB of a 24 GB machine.
    mlx_rs::memory::set_cache_limit(CACHE_LIMIT_BYTES).map_err(|e| anyhow::anyhow!("{e}"))?;
    mlx_rs::random::seed(SEED).map_err(|e| anyhow::anyhow!("{e}"))?;

    let model = MlxModel::load(
        &config.model_dir,
        model_cfg,
        config.lora_rank,
        config.lora_alpha,
    )
    .context("loading base weights on MLX")?;
    let mut lora = model.init_lora(config.lora_rank)?;
    let mut optimizer =
        AdamW::new(&lora, config.learning_rate).map_err(|e| anyhow::anyhow!("{e}"))?;

    let batches: Vec<Option<Batch>> = examples
        .iter()
        .map(|e| Batch::new(&e.tokens, &e.mask))
        .collect();
    let total_steps = examples.len();
    let debug = std::env::var("ONDE_FINETUNE_GRAD_DEBUG").is_ok();
    let started = Instant::now();
    let mut tokens_seen = 0usize;

    for epoch in 0..config.epochs {
        for (step, batch) in batches.iter().enumerate() {
            let progress = |loss: f32| FineTuneProgress::Training {
                epoch: epoch + 1,
                total_epochs: config.epochs,
                step: step + 1,
                total_steps,
                loss,
            };
            let Some(batch) = batch else {
                eprintln!(
                    "[finetune] WARNING: no masked tokens at epoch {} step {}, skipping",
                    epoch + 1,
                    step + 1
                );
                let _ = tx.send(progress(0.0));
                continue;
            };

            let (loss, grads) = model
                .loss_and_grads(&lora, batch)
                .map_err(|e| anyhow::anyhow!("forward/backward: {e}"))?;
            let clean: Params = grads
                .iter()
                .map(|(k, g)| Ok((k.clone(), sanitize(g)?)))
                .collect::<Mlx<_>>()
                .map_err(|e| anyhow::anyhow!("sanitizing gradient: {e}"))?;
            let loss_val = loss.item_exact::<f32>();
            tokens_seen += batch.seq_len;

            if loss_val.is_nan() || loss_val.is_infinite() {
                eprintln!(
                    "[finetune] WARNING: loss is {} at epoch {} step {}, skipping",
                    loss_val,
                    epoch + 1,
                    step + 1
                );
                let _ = tx.send(progress(loss_val));
                continue;
            }

            let (max_abs, total_norm) = grad_stats(&clean)?;
            if debug {
                eprintln!(
                    "[finetune] grad-debug epoch {} step {} seq_len {} loss {:.4} max_abs {:.3e} norm {:.3e}",
                    epoch + 1,
                    step + 1,
                    batch.seq_len,
                    loss_val,
                    max_abs,
                    total_norm
                );
            }
            if total_norm == 0.0 || !total_norm.is_finite() {
                eprintln!(
                    "[finetune] WARNING: global grad norm is {} at epoch {} step {} (seq_len {}), skipping update",
                    total_norm,
                    epoch + 1,
                    step + 1,
                    batch.seq_len
                );
                continue;
            }

            let mut clean = clean;
            clip(&mut clean, total_norm).map_err(|e| anyhow::anyhow!("clipping gradient: {e}"))?;
            optimizer
                .step(&mut lora, &clean)
                .map_err(|e| anyhow::anyhow!("optimizer step: {e}"))?;
            // One eval per step keeps the lazy graph bounded.
            eval(
                lora.values()
                    .chain(optimizer.m.values())
                    .chain(optimizer.v.values())
                    .chain(std::iter::once(&loss)),
            )
            .map_err(|e| anyhow::anyhow!("evaluating step: {e}"))?;

            let _ = tx.send(progress(loss_val));
        }
    }

    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "[finetune] mlx: {tokens_seen} tokens in {secs:.1}s ({:.0} tok/s)",
        tokens_seen as f64 / secs.max(1e-9)
    );

    let _ = tx.send(FineTuneProgress::Saving);
    let adapter_path = save_adapter(&lora, &config.output_dir)?;

    // Hand the GPU memory back: the caller may load a model in this process next
    // (the e2e test loads the exported GGUF in mistral.rs right after training).
    let peak = mlx_rs::memory::peak_memory().unwrap_or(0);
    drop((model, lora, optimizer, batches));
    let _ = mlx_rs::memory::clear_cache();
    eprintln!(
        "[finetune] mlx: peak {:.1} GiB, still active after cleanup {:.2} GiB",
        peak as f64 / (1u64 << 30) as f64,
        mlx_rs::memory::active_memory().unwrap_or(0) as f64 / (1u64 << 30) as f64
    );
    let _ = tx.send(FineTuneProgress::Done { adapter_path });
    Ok(())
}

/// Copy an f32 array to host memory. Flattening first guarantees a contiguous
/// layout (`as_slice` refuses strided views such as transposed gradients).
fn host_f32(a: &Array) -> Mlx<Vec<f32>> {
    let flat = a.reshape(&[-1])?;
    flat.eval()?;
    Ok(flat.as_slice::<f32>().to_vec())
}

/// Copy each adapter array to host memory and write it with candle's safetensors
/// writer, under `model.layers.N.self_attn.{q,v}_proj.lora_{a,b}`.
fn save_adapter(lora: &Params, output_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("creating output directory {output_dir:?}"))?;
    let mut tensors: HashMap<String, Tensor> = HashMap::new();
    for (k, a) in lora {
        let data = host_f32(a).map_err(|e| anyhow::anyhow!("{e}"))?;
        let shape: Vec<usize> = a.shape().iter().map(|&d| d as usize).collect();
        let tensor = Tensor::from_vec(data, shape, &Device::Cpu)
            .with_context(|| format!("building tensor {k}"))?;
        tensors.insert(k.to_string(), tensor);
    }
    let path = output_dir.join("lora_adapter.safetensors");
    candle_core::safetensors::save(&tensors, &path).with_context(|| format!("writing {path:?}"))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finetune::LoraQwenModel;
    use candle_core::DType;
    use candle_nn::{AdamW as CandleAdamW, Optimizer, ParamsAdamW, VarBuilder};

    const RANK: usize = 4;
    const ALPHA: f32 = 8.0;

    /// Deterministic pseudo-random values in `[-scale, scale]`.
    fn rnd(n: usize, seed: u64, scale: f32) -> Vec<f32> {
        let mut x = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (0..n)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (((x >> 33) as f64 / (1u64 << 31) as f64) as f32 * 2.0 - 1.0) * scale
            })
            .collect()
    }

    struct Tiny {
        cfg: ModelConfig,
        tensors: Vec<(String, Vec<f32>, Vec<usize>)>,
    }

    fn tiny(qwen3: bool) -> Tiny {
        let (hidden, heads, kv, inter, vocab, layers) =
            (64usize, 4usize, 2usize, 128usize, 100usize, 2usize);
        let head_dim = if qwen3 { 24 } else { hidden / heads };
        let cfg = ModelConfig {
            hidden_size: hidden,
            num_hidden_layers: layers,
            num_attention_heads: heads,
            num_key_value_heads: Some(kv),
            head_dim: qwen3.then_some(head_dim),
            intermediate_size: inter,
            vocab_size: vocab,
            rms_norm_eps: Some(1e-6),
            rope_theta: Some(10_000.0),
            tie_word_embeddings: !qwen3,
        };
        let mut t: Vec<(String, Vec<f32>, Vec<usize>)> = Vec::new();
        let mut seed = 1u64;
        let mut add = |name: String, shape: Vec<usize>, scale: f32, around_one: bool| {
            seed += 1;
            let n: usize = shape.iter().product();
            let mut v = rnd(n, seed, scale);
            if around_one {
                v.iter_mut().for_each(|x| *x += 1.0);
            }
            t.push((name, v, shape));
        };
        add(
            "model.embed_tokens.weight".into(),
            vec![vocab, hidden],
            0.2,
            false,
        );
        add("model.norm.weight".into(), vec![hidden], 0.1, true);
        if qwen3 {
            add("lm_head.weight".into(), vec![vocab, hidden], 0.2, false);
        }
        for i in 0..layers {
            let p = format!("model.layers.{i}");
            add(
                format!("{p}.self_attn.q_proj.weight"),
                vec![heads * head_dim, hidden],
                0.15,
                false,
            );
            add(
                format!("{p}.self_attn.k_proj.weight"),
                vec![kv * head_dim, hidden],
                0.15,
                false,
            );
            add(
                format!("{p}.self_attn.v_proj.weight"),
                vec![kv * head_dim, hidden],
                0.15,
                false,
            );
            add(
                format!("{p}.self_attn.o_proj.weight"),
                vec![hidden, heads * head_dim],
                0.15,
                false,
            );
            if qwen3 {
                add(
                    format!("{p}.self_attn.q_norm.weight"),
                    vec![head_dim],
                    0.1,
                    true,
                );
                add(
                    format!("{p}.self_attn.k_norm.weight"),
                    vec![head_dim],
                    0.1,
                    true,
                );
            } else {
                add(
                    format!("{p}.self_attn.q_proj.bias"),
                    vec![heads * head_dim],
                    0.3,
                    false,
                );
                add(
                    format!("{p}.self_attn.k_proj.bias"),
                    vec![kv * head_dim],
                    0.3,
                    false,
                );
                add(
                    format!("{p}.self_attn.v_proj.bias"),
                    vec![kv * head_dim],
                    0.3,
                    false,
                );
            }
            add(
                format!("{p}.mlp.gate_proj.weight"),
                vec![inter, hidden],
                0.15,
                false,
            );
            add(
                format!("{p}.mlp.up_proj.weight"),
                vec![inter, hidden],
                0.15,
                false,
            );
            add(
                format!("{p}.mlp.down_proj.weight"),
                vec![hidden, inter],
                0.15,
                false,
            );
            add(
                format!("{p}.input_layernorm.weight"),
                vec![hidden],
                0.1,
                true,
            );
            add(
                format!("{p}.post_attention_layernorm.weight"),
                vec![hidden],
                0.1,
                true,
            );
        }
        Tiny { cfg, tensors: t }
    }

    fn rel(a: &[f32], b: &[f32]) -> f64 {
        let num: f64 = a.iter().zip(b).map(|(x, y)| ((x - y) as f64).powi(2)).sum();
        let den: f64 = b.iter().map(|y| (*y as f64).powi(2)).sum();
        num.sqrt() / den.sqrt().max(1e-30)
    }

    fn mlx_vec(a: &Array) -> Vec<f32> {
        host_f32(a).unwrap()
    }

    /// Deterministic non-zero LoRA values; B non-zero so A receives gradient.
    fn lora_values(model: &MlxModel) -> Vec<(String, Vec<f32>, Vec<usize>)> {
        let mut out = Vec::new();
        let mut seed = 100u64;
        for i in 0..model.dims.layers {
            for name in ["q_proj", "v_proj"] {
                let w = model.w(&format!("model.layers.{i}.self_attn.{name}.weight"));
                let (o, inp) = (w.shape()[0] as usize, w.shape()[1] as usize);
                seed += 1;
                out.push((
                    format!("model.layers.{i}.self_attn.{name}.lora_a"),
                    rnd(RANK * inp, seed, 0.3),
                    vec![RANK, inp],
                ));
                seed += 1;
                out.push((
                    format!("model.layers.{i}.self_attn.{name}.lora_b"),
                    rnd(o * RANK, seed, 0.1),
                    vec![o, RANK],
                ));
            }
        }
        out
    }

    /// Candle side: loss, then gradients in `lora_values` order.
    struct CandleRun {
        model: LoraQwenModel,
        vars: Vec<candle_core::Var>,
    }

    fn candle_model(t: &Tiny, lora: &[(String, Vec<f32>, Vec<usize>)]) -> CandleRun {
        let dev = Device::Cpu;
        let map: HashMap<String, Tensor> = t
            .tensors
            .iter()
            .map(|(k, v, s)| {
                (
                    k.clone(),
                    Tensor::from_vec(v.clone(), s.clone(), &dev).unwrap(),
                )
            })
            .collect();
        let vb = VarBuilder::from_tensors(map, DType::F32, &dev);
        let model = LoraQwenModel::load(vb, &t.cfg, RANK, ALPHA, &dev).unwrap();
        let vars = model.lora_vars();
        // lora_vars order: per layer q_a, q_b, v_a, v_b, matching `lora_values`.
        assert_eq!(vars.len(), lora.len());
        for (var, (_, vals, shape)) in vars.iter().zip(lora) {
            var.set(&Tensor::from_vec(vals.clone(), shape.clone(), &dev).unwrap())
                .unwrap();
        }
        CandleRun { model, vars }
    }

    fn candle_loss(run: &CandleRun, cfg: &ModelConfig, ids: &[u32], mask: &[bool]) -> Tensor {
        let dev = Device::Cpu;
        let n = ids.len();
        let input = Tensor::from_slice(ids, (1, n), &dev).unwrap();
        let logits = run.model.forward(&input).unwrap();
        let shifted = logits
            .narrow(1, 0, n - 1)
            .unwrap()
            .reshape((n - 1, cfg.vocab_size))
            .unwrap();
        let targets = Tensor::from_slice(&ids[1..], (n - 1,), &dev).unwrap();
        let keep: Vec<u32> = (0..n - 1)
            .filter(|&j| mask[j + 1])
            .map(|j| j as u32)
            .collect();
        let keep = Tensor::from_slice(keep.as_slice(), (keep.len(),), &dev).unwrap();
        candle_nn::loss::cross_entropy(
            &shifted.index_select(&keep, 0).unwrap(),
            &targets.index_select(&keep, 0).unwrap(),
        )
        .unwrap()
    }

    fn flat(t: &Tensor) -> Vec<f32> {
        t.flatten_all().unwrap().to_vec1::<f32>().unwrap()
    }

    fn check_against_candle(qwen3: bool) {
        let t = tiny(qwen3);
        let all: HashMap<String, Array> = t
            .tensors
            .iter()
            .map(|(k, v, s)| {
                let shape: Vec<i32> = s.iter().map(|&d| d as i32).collect();
                (k.clone(), Array::from_slice(v, &shape))
            })
            .collect();
        let model = MlxModel::from_weights(&t.cfg, all, RANK, ALPHA).unwrap();
        let lora_vals = lora_values(&model);
        let mut lora = Params::new();
        for (k, v, s) in &lora_vals {
            let shape: Vec<i32> = s.iter().map(|&d| d as i32).collect();
            lora.insert(k.as_str().into(), Array::from_slice(v, &shape));
        }

        let ids: Vec<u32> = (0..12).map(|i| (i * 7 + 3) % 100).collect();
        // Partial mask: the first five tokens carry no loss.
        let mask: Vec<bool> = (0..12).map(|i| i >= 5).collect();

        let cr = candle_model(&t, &lora_vals);
        let c_loss = candle_loss(&cr, &t.cfg, &ids, &mask);
        let c_loss_val = c_loss.to_scalar::<f32>().unwrap();
        let mut c_grads = c_loss.backward().unwrap();
        let c_grad_tensors: Vec<Tensor> = cr
            .vars
            .iter()
            .map(|v| c_grads.get(v.as_tensor()).expect("candle grad").clone())
            .collect();

        let batch = Batch::new(&ids, &mask).unwrap();
        let (m_loss, m_grads) = model.loss_and_grads(&lora, &batch).unwrap();
        let m_loss_val = m_loss.item_exact::<f32>();
        let loss_rel = ((m_loss_val - c_loss_val).abs() / c_loss_val.abs()) as f64;
        assert!(
            loss_rel < 1e-3,
            "loss mlx {m_loss_val} vs candle {c_loss_val}"
        );

        for (var, (key, _, _)) in cr.vars.iter().zip(&lora_vals) {
            let cg = flat(c_grads.get(var.as_tensor()).expect("candle grad"));
            let mg = mlx_vec(&m_grads[&**key]);
            assert!(
                cg.iter().any(|x| *x != 0.0),
                "{key}: reference gradient is all zero"
            );
            let d = rel(&mg, &cg);
            assert!(d < 1e-3, "{key}: gradient relative diff {d:.3e}");
        }

        // Two AdamW steps (the second with different gradients, to exercise the
        // bias correction and the moments) on each side.
        let lr = 1e-2;
        let mut copt = CandleAdamW::new(
            cr.vars.clone(),
            ParamsAdamW {
                lr,
                ..ParamsAdamW::default()
            },
        )
        .unwrap();
        let mut mopt = AdamW::new(&lora, lr).unwrap();
        let mut mparams = lora.clone();
        for round in 0..2 {
            let factor = if round == 0 { 1.0 } else { 0.5 };
            // Reuse the gradients from before the first update: backward through
            // the old graph after `Var::set` would mix new weights with old
            // activations.
            for (var, g) in cr.vars.iter().zip(&c_grad_tensors) {
                c_grads.insert(var.as_tensor(), (g * factor).unwrap());
            }
            copt.step(&c_grads).unwrap();
            let scaled: Params = m_grads
                .iter()
                .map(|(k, g)| (k.clone(), g.multiply(scalar(factor)).unwrap()))
                .collect();
            mopt.step(&mut mparams, &scaled).unwrap();
        }
        for (var, (key, _, _)) in cr.vars.iter().zip(&lora_vals) {
            let c = flat(var.as_tensor());
            let m = mlx_vec(&mparams[&**key]);
            let d = rel(&m, &c);
            assert!(d < 1e-3, "{key}: updated parameter relative diff {d:.3e}");
        }
    }

    #[test]
    fn mlx_matches_candle_qwen2_with_biases() {
        check_against_candle(false);
    }

    #[test]
    fn mlx_matches_candle_qwen3_qk_norm() {
        check_against_candle(true);
    }

    #[test]
    fn sanitize_zeroes_non_finite_and_clip_bounds_the_norm() {
        let g = Array::from_slice(&[1.0f32, f32::NAN, f32::INFINITY, -3.0, 1e31], &[5]);
        let clean = sanitize(&g).unwrap();
        assert_eq!(mlx_vec(&clean), vec![1.0, 0.0, 0.0, -3.0, 0.0]);

        let mut p = Params::new();
        p.insert("a".into(), Array::from_slice(&[30.0f32, 40.0], &[2]));
        let (max_abs, norm) = grad_stats(&p).unwrap();
        assert_eq!(max_abs, 40.0);
        assert!((norm - 50.0).abs() < 1e-3);
        clip(&mut p, norm).unwrap();
        let (_, after) = grad_stats(&p).unwrap();
        assert!((after - 1.0).abs() < 1e-3, "clipped norm {after}");

        let mut zero = Params::new();
        zero.insert("a".into(), Array::from_slice(&[0.0f32; 3], &[3]));
        assert_eq!(grad_stats(&zero).unwrap().1, 0.0);
    }

    #[test]
    fn adamw_decays_weights_decoupled_from_the_gradient() {
        let mut params = Params::new();
        params.insert("w".into(), Array::from_slice(&[1.0f32, -2.0], &[2]));
        let mut grads = Params::new();
        grads.insert("w".into(), Array::from_slice(&[0.0f32, 0.0], &[2]));
        let mut opt = AdamW::new(&params, 0.5).unwrap();
        opt.step(&mut params, &grads).unwrap();
        // zero gradient: only decay acts, theta * (1 - lr * 0.01).
        let w = mlx_vec(&params["w"]);
        assert!(
            (w[0] - 0.995).abs() < 1e-6 && (w[1] + 1.99).abs() < 1e-6,
            "{w:?}"
        );
    }

    #[test]
    fn batch_keeps_positions_where_the_shifted_mask_is_true() {
        let tokens = [10u32, 11, 12, 13, 14];
        let mask = [false, false, true, true, false];
        let b = Batch::new(&tokens, &mask).unwrap();
        // mask[1..] = [false, true, true, false] -> predict tokens 12 and 13.
        assert_eq!(mlx_vec_i32(&b.keep), vec![1, 2]);
        assert_eq!(mlx_vec_i32(&b.targets), vec![12, 13]);
        assert!(Batch::new(&tokens, &[true, false, false, false, false]).is_none());
    }

    fn mlx_vec_i32(a: &Array) -> Vec<i32> {
        eval(std::iter::once(a)).unwrap();
        a.as_slice::<i32>().to_vec()
    }

    // -- real model --------------------------------------------------------

    /// Compare against the CPU reference written by
    /// `finetune::spike::spike_dump_reference` on the same model:
    ///
    /// ```sh
    /// ONDE_SPIKE_MODEL_DIR=<qwen2.5-1.5b dir> cargo test --release \
    ///   finetune::spike::spike_dump_reference -- --ignored --nocapture
    /// ONDE_SPIKE_MODEL_DIR=<same> cargo test --release \
    ///   finetune::mlx::tests::real_model_matches_candle_reference -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn real_model_matches_candle_reference() {
        let dir =
            PathBuf::from(std::env::var("ONDE_SPIKE_MODEL_DIR").expect("set ONDE_SPIKE_MODEL_DIR"));
        let ref_path =
            std::env::var("ONDE_SPIKE_DUMP").unwrap_or("/tmp/lora_ref.safetensors".into());
        let cfg: ModelConfig =
            serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap())
                .unwrap();
        let model = MlxModel::load(&dir, &cfg, 8, 16.0).unwrap();

        let reference = Array::load_safetensors(&ref_path).unwrap();
        let ids: Vec<u32> = reference["ids"]
            .as_slice::<f32>()
            .iter()
            .map(|x| *x as u32)
            .collect();
        let n = ids.len();
        // spike's loss keeps shifted positions n/3.., i.e. mask[k] for k > n/3.
        let mask: Vec<bool> = (0..n).map(|k| k > n / 3).collect();
        let batch = Batch::new(&ids, &mask).unwrap();
        let mut lora = Params::new();
        for (k, v) in &reference {
            if k.starts_with("model.layers.") {
                lora.insert(k.as_str().into(), v.clone());
            }
        }

        let t = Instant::now();
        let (loss, grads) = model.loss_and_grads(&lora, &batch).unwrap();
        eval(grads.values().chain(std::iter::once(&loss))).unwrap();
        let ref_loss = reference["loss"].item_exact::<f32>();
        let mlx_loss = loss.item_exact::<f32>();
        eprintln!(
            "seq {n}: loss ref {ref_loss:.5} mlx {mlx_loss:.5} ({} ms)",
            t.elapsed().as_millis()
        );
        assert!(((mlx_loss - ref_loss) / ref_loss).abs() < 1e-3);

        let (mut worst, mut worst_key, mut compared) = (0f64, String::new(), 0);
        let mut biggest = (0f64, String::new());
        for (k, g) in &grads {
            let r = mlx_vec(&reference[&format!("grad.{k}")]);
            let rn: f64 = r.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
            if rn == 0.0 {
                continue;
            }
            compared += 1;
            let d = rel(&mlx_vec(g), &r);
            if d > worst {
                worst = d;
                worst_key = k.to_string();
            }
            if rn > biggest.0 {
                biggest = (rn, k.to_string());
            }
        }
        eprintln!(
            "{compared} nonzero tensors compared, worst relative diff {worst:.3e} ({worst_key})"
        );
        assert!(
            worst < 1e-3,
            "worst gradient diff {worst:.3e} at {worst_key}"
        );

        // Finite difference along the unit gradient of the largest tensor.
        let key = biggest.1;
        let g = &grads[&*key];
        let gn = (mlx_vec(g).iter().map(|x| (*x as f64).powi(2)).sum::<f64>()).sqrt();
        let dir_unit = g.divide(scalar(gn)).unwrap();
        let eps = 1e-4f32;
        let at = |s: f32| {
            let mut p = lora.clone();
            let shifted = p[&*key]
                .add(dir_unit.multiply(Array::from_f32(s)).unwrap())
                .unwrap();
            p.insert(key.as_str().into(), shifted);
            model.loss(&p, &batch).unwrap().item_exact::<f32>() as f64
        };
        let fd = (at(eps) - at(-eps)) / (2.0 * eps as f64);
        let ratio = fd / gn;
        eprintln!("finite difference on {key}: fd {fd:.5e} vs |grad| {gn:.5e} (ratio {ratio:.4})");
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "finite-difference ratio {ratio}"
        );
    }
}
