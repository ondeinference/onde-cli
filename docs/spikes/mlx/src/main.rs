//! Backend C spike: Qwen 3 LoRA forward/backward on MLX, same layout and
//! adapter tensor names as the candle trainer in onde-cli (`finetune.rs`).
//!
//!   mlx-spike grad            compare loss and LoRA gradients with /tmp/lora_ref.safetensors
//!   mlx-spike bench           overfit a fixed batch and report tok/s
//! Env: ONDE_SPIKE_MODEL_DIR, ONDE_SPIKE_SEQ, ONDE_SPIKE_EXAMPLES, ONDE_SPIKE_EPOCHS, ONDE_SPIKE_LR.

use std::{collections::HashMap, path::PathBuf, rc::Rc, time::Instant};

use mlx_rs::{
    fast::{self, ScaledDotProductAttentionMask},
    nn::log_softmax,
    ops,
    transforms::{eval, keyed_value_and_grad},
    Array,
};

type Params = HashMap<Rc<str>, Array>;

struct Cfg {
    hidden: i32,
    heads: i32,
    kv_heads: i32,
    head_dim: i32,
    layers: usize,
    eps: f32,
    theta: f32,
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn model_dir() -> PathBuf {
    PathBuf::from(std::env::var("ONDE_SPIKE_MODEL_DIR").expect("set ONDE_SPIKE_MODEL_DIR"))
}

fn load_cfg() -> Cfg {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(model_dir().join("config.json")).unwrap()).unwrap();
    let i = |k: &str| v[k].as_i64().unwrap() as i32;
    Cfg {
        hidden: i("hidden_size"),
        heads: i("num_attention_heads"),
        kv_heads: i("num_key_value_heads"),
        head_dim: i("head_dim"),
        layers: i("num_hidden_layers") as usize,
        eps: v["rms_norm_eps"].as_f64().unwrap() as f32,
        theta: v["rope_theta"].as_f64().unwrap() as f32,
    }
}

fn load_weights() -> HashMap<String, Array> {
    let raw = Array::load_safetensors(model_dir().join("model.safetensors")).unwrap();
    raw.into_iter().map(|(k, v)| (k, v.as_type::<f32>().unwrap())).collect()
}

const RANK: f32 = 8.0;
const ALPHA: f32 = 16.0;

fn f(x: f32) -> Array {
    Array::from_f32(x)
}

/// `y = x W^T + (x A^T) B^T * scale`
fn lora_linear(x: &Array, w: &Array, a: &Array, b: &Array) -> Array {
    let base = x.matmul(w.t()).unwrap();
    let lora = x.matmul(a.t()).unwrap().matmul(b.t()).unwrap();
    base.add(lora.multiply(f(ALPHA / RANK)).unwrap()).unwrap()
}

fn frozen(x: &Array, w: &Array) -> Array {
    x.matmul(w.t()).unwrap()
}

fn forward(cfg: &Cfg, w: &HashMap<String, Array>, lora: &Params, ids: &Array) -> Array {
    let n = ids.shape()[0];
    let mut h = w["model.embed_tokens.weight"].take_axis(ids, 0).unwrap(); // [n, hidden]
    for i in 0..cfg.layers {
        let p = format!("model.layers.{i}");
        let g = |s: &str| &w[&format!("{p}.{s}")];
        let l = |s: &str| &lora[&*format!("{p}.self_attn.{s}")];
        let normed = fast::rms_norm(&h, Some(g("input_layernorm.weight")), cfg.eps).unwrap();
        let q = lora_linear(&normed, g("self_attn.q_proj.weight"), l("q_proj.lora_a"), l("q_proj.lora_b"));
        let k = frozen(&normed, g("self_attn.k_proj.weight"));
        let v = lora_linear(&normed, g("self_attn.v_proj.weight"), l("v_proj.lora_a"), l("v_proj.lora_b"));
        let heads = |t: &Array, nh: i32| {
            t.reshape(&[1, n, nh, cfg.head_dim]).unwrap().transpose_axes(&[0, 2, 1, 3]).unwrap()
        };
        let (q, k, v) = (heads(&q, cfg.heads), heads(&k, cfg.kv_heads), heads(&v, cfg.kv_heads));
        let q = fast::rms_norm(&q, Some(g("self_attn.q_norm.weight")), cfg.eps).unwrap();
        let k = fast::rms_norm(&k, Some(g("self_attn.k_norm.weight")), cfg.eps).unwrap();
        let q = fast::rope(&q, cfg.head_dim, false, Some(cfg.theta), 1.0, 0, None).unwrap();
        let k = fast::rope(&k, cfg.head_dim, false, Some(cfg.theta), 1.0, 0, None).unwrap();
        let scale = 1.0 / (cfg.head_dim as f32).sqrt();
        let o = fast::scaled_dot_product_attention(&q, &k, &v, scale, ScaledDotProductAttentionMask::Causal, None)
            .unwrap();
        let o = o.transpose_axes(&[0, 2, 1, 3]).unwrap().reshape(&[n, cfg.heads * cfg.head_dim]).unwrap();
        let o = frozen(&o, g("self_attn.o_proj.weight"));
        h = h.add(o).unwrap();
        let normed2 = fast::rms_norm(&h, Some(g("post_attention_layernorm.weight")), cfg.eps).unwrap();
        let gate = mlx_rs::nn::silu(frozen(&normed2, g("mlp.gate_proj.weight"))).unwrap();
        let up = frozen(&normed2, g("mlp.up_proj.weight"));
        let down = frozen(&gate.multiply(up).unwrap(), g("mlp.down_proj.weight"));
        h = h.add(down).unwrap();
    }
    let h = fast::rms_norm(&h, Some(&w["model.norm.weight"]), cfg.eps).unwrap();
    h.matmul(w["model.embed_tokens.weight"].t()).unwrap() // tied lm_head
}

/// Same shape as the candle trainer's loss: first third masked, mean NLL.
fn loss_fn(cfg: &Cfg, w: &HashMap<String, Array>, lora: &Params, ids: &Array) -> Array {
    let n = ids.shape()[0];
    let logits = forward(cfg, w, lora, ids);
    let keep = Array::from_slice(&((n / 3)..(n - 1)).collect::<Vec<i32>>(), &[n - 1 - n / 3]);
    let tail = Array::from_slice(&(1..n).collect::<Vec<i32>>(), &[n - 1]);
    let targets_all = ids.take_axis(&tail, 0).unwrap();
    let sel = logits.take_axis(&keep, 0).unwrap();
    let tg = targets_all.take_axis(&keep, 0).unwrap().reshape(&[-1, 1]).unwrap();
    let lp = log_softmax(&sel, -1).unwrap();
    let picked = lp.take_along_axis(&tg, -1).unwrap();
    picked.mean(None).unwrap().negative().unwrap()
}

fn value_and_grads(cfg: &Cfg, w: &HashMap<String, Array>, lora: Params, ids: &Array) -> (Array, Params) {
    let fun = |p: Params, _: i32| -> Vec<Array> { vec![loss_fn(cfg, w, &p, ids)] };
    let mut vg = keyed_value_and_grad(fun);
    let (v, g) = vg(lora, 0).unwrap();
    (v.into_iter().next().unwrap(), g)
}

fn norm(a: &Array) -> f64 {
    let sq = a.multiply(a).unwrap().sum(None).unwrap();
    (sq.item::<f32>() as f64).sqrt()
}

fn rel(a: &Array, b: &Array) -> f64 {
    norm(&a.subtract(b).unwrap()) / norm(b).max(1e-30)
}

fn grad_mode(cfg: &Cfg, w: &HashMap<String, Array>) {
    let reference = Array::load_safetensors("/tmp/lora_ref.safetensors").unwrap();
    let ids: Vec<i32> = reference["ids"].as_slice::<f32>().iter().map(|x| *x as i32).collect();
    let ids = Array::from_slice(&ids, &[ids.len() as i32]);
    let mut lora: Params = HashMap::new();
    for (k, v) in &reference {
        if k.starts_with("model.layers.") {
            lora.insert(k.as_str().into(), v.clone());
        }
    }
    let t = Instant::now();
    let (loss, grads) = value_and_grads(cfg, w, lora.clone(), &ids);
    eval(grads.values().chain(std::iter::once(&loss))).unwrap();
    let ms = t.elapsed().as_millis();
    let ref_loss = reference["loss"].item::<f32>();
    println!("seq {}: loss ref {ref_loss:.5} mlx {:.5}  fwd+bwd {ms} ms", ids.shape()[0], loss.item::<f32>());
    let (mut worst, mut worst_key, mut compared) = (0f64, String::new(), 0);
    let mut biggest = (0f64, String::new());
    for (k, g) in &grads {
        let r = &reference[&format!("grad.{k}")];
        let rn = norm(r);
        if rn == 0.0 {
            continue;
        }
        compared += 1;
        let d = rel(g, r);
        if d > worst {
            worst = d;
            worst_key = k.to_string();
        }
        if rn > biggest.0 {
            biggest = (rn, k.to_string());
        }
    }
    println!("{compared} nonzero tensors compared, worst relative diff {worst:.3e} ({worst_key})");
    let key = biggest.1;
    let g = &grads[&*key];
    let gn = norm(g);
    let dir = g.divide(f(gn as f32)).unwrap();
    for eps in [1e-3f32, 1e-4, 1e-5] {
        let at = |s: f32| {
            let mut p = lora.clone();
            let shifted = p[&*key].add(dir.multiply(f(s)).unwrap()).unwrap();
            p.insert(key.as_str().into(), shifted);
            loss_fn(cfg, w, &p, &ids).item::<f32>() as f64
        };
        let fd = (at(eps) - at(-eps)) / (2.0 * eps as f64);
        println!("finite difference on {key} eps {eps:e}: fd {fd:.5e} vs |grad| {gn:.5e} (ratio {:.4})", fd / gn);
    }
}

fn batch(seq: usize, n: usize) -> Vec<Vec<i32>> {
    let tok = tokenizers::Tokenizer::from_file(model_dir().join("tokenizer.json")).unwrap();
    let topics = [
        "the harbour at dawn", "a bread recipe", "tide tables", "a bicycle repair",
        "a train timetable", "garden soil", "a chess opening", "winter clothing",
        "a lighthouse keeper", "river maps", "a tea ceremony", "a pottery kiln",
        "a market stall", "a sailing knot", "a night sky chart", "a library card",
        "a violin lesson", "a mountain hut", "a postage stamp", "a weather report",
    ];
    (0..n)
        .map(|i| {
            let line = format!(
                "Notes on {}: it matters how it is described, and the description should be clear, short and accurate. ",
                topics[i % topics.len()]
            );
            let mut text = String::new();
            let mut ids: Vec<u32> = Vec::new();
            while ids.len() < seq {
                text.push_str(&line);
                ids = tok.encode(text.as_str(), false).unwrap().get_ids().to_vec();
            }
            ids.truncate(seq);
            ids.into_iter().map(|x| x as i32).collect()
        })
        .collect()
}

fn bench_mode(cfg: &Cfg, w: &HashMap<String, Array>) {
    let seq = env_usize("ONDE_SPIKE_SEQ", 256);
    let n = env_usize("ONDE_SPIKE_EXAMPLES", 20);
    let epochs = env_usize("ONDE_SPIKE_EPOCHS", 3);
    let lr: f32 = std::env::var("ONDE_SPIKE_LR").ok().and_then(|v| v.parse().ok()).unwrap_or(1e-3);
    let data = batch(seq, n);
    let mut lora: Params = HashMap::new();
    let (hd_q, hd_kv) = (cfg.heads * cfg.head_dim, cfg.kv_heads * cfg.head_dim);
    for i in 0..cfg.layers {
        for (name, out) in [("q_proj", hd_q), ("v_proj", hd_kv)] {
            let a = mlx_rs::random::normal::<f32>(&[RANK as i32, cfg.hidden], 0.0, 1.0 / RANK.sqrt(), None).unwrap();
            let b = ops::zeros::<f32>(&[out, RANK as i32]).unwrap();
            lora.insert(format!("model.layers.{i}.self_attn.{name}.lora_a").as_str().into(), a);
            lora.insert(format!("model.layers.{i}.self_attn.{name}.lora_b").as_str().into(), b);
        }
    }
    eval(lora.values()).unwrap();
    let mut m: Params = lora.iter().map(|(k, v)| (k.clone(), ops::zeros_like(v).unwrap())).collect();
    let mut s: Params = m.clone();
    let (b1, b2, adam_eps) = (0.9f32, 0.999f32, 1e-8f32);
    let start = Instant::now();
    let (mut tokens, mut step) = (0usize, 0i32);
    for epoch in 0..epochs {
        let mut total = 0f64;
        for ids in &data {
            let ids_a = Array::from_slice(ids, &[ids.len() as i32]);
            let (loss, mut grads) = value_and_grads(cfg, w, lora.clone(), &ids_a);
            let mut sq = f(0.0);
            for g in grads.values() {
                sq = sq.add(g.multiply(g).unwrap().sum(None).unwrap()).unwrap();
            }
            let gn = sq.sqrt().unwrap();
            let coef = ops::minimum(f(1.0), f(1.0).divide(gn.add(f(1e-6)).unwrap()).unwrap()).unwrap();
            for g in grads.values_mut() {
                *g = g.multiply(&coef).unwrap();
            }
            step += 1;
            let c1 = 1.0 - b1.powi(step);
            let c2 = 1.0 - b2.powi(step);
            for (k, g) in &grads {
                let mk = m[k].multiply(f(b1)).unwrap().add(g.multiply(f(1.0 - b1)).unwrap()).unwrap();
                let sk = s[k]
                    .multiply(f(b2))
                    .unwrap()
                    .add(g.multiply(g).unwrap().multiply(f(1.0 - b2)).unwrap())
                    .unwrap();
                let denom = sk.divide(f(c2)).unwrap().sqrt().unwrap().add(f(adam_eps)).unwrap();
                let upd = mk.divide(f(c1)).unwrap().divide(denom).unwrap();
                let p = lora[k].subtract(upd.multiply(f(lr)).unwrap()).unwrap();
                m.insert(k.clone(), mk);
                s.insert(k.clone(), sk);
                lora.insert(k.clone(), p);
            }
            eval(lora.values().chain(m.values()).chain(s.values()).chain(std::iter::once(&loss))).unwrap();
            total += loss.item::<f32>() as f64;
            tokens += ids.len();
        }
        let secs = start.elapsed().as_secs_f64();
        println!(
            "epoch {} mean loss {:.4}  elapsed {:.1}s  {:.0} tok/s",
            epoch + 1,
            total / data.len() as f64,
            secs,
            tokens as f64 / secs
        );
    }
}

fn main() {
    let cfg = load_cfg();
    let w = load_weights();
    match std::env::args().nth(1).as_deref() {
        Some("grad") => grad_mode(&cfg, &w),
        Some("bench") => bench_mode(&cfg, &w),
        _ => eprintln!("usage: mlx-spike grad|bench"),
    }
}
