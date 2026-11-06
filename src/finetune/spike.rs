//! Backend spike: compare Metal against CPU for the LoRA trainer.
//!
//! All tests are `#[ignore]` and need `ONDE_SPIKE_MODEL_DIR` (a Qwen 3
//! checkpoint directory). Run with `--ignored --nocapture`.

use super::*;

fn model_dir() -> PathBuf {
    PathBuf::from(std::env::var("ONDE_SPIKE_MODEL_DIR").expect("set ONDE_SPIKE_MODEL_DIR"))
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn build(device: &Device) -> (LoraQwenModel, ModelConfig) {
    let dir = model_dir();
    let cfg: ModelConfig =
        serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[dir.join("model.safetensors")], DType::F32, device)
            .unwrap()
    };
    let model = LoraQwenModel::load(vb, &cfg, 8, 16.0, device).unwrap();
    (model, cfg)
}

fn device_from_env() -> Device {
    match std::env::var("ONDE_SPIKE_DEVICE").as_deref() {
        Ok("metal") => Device::new_metal(0).expect("metal device"),
        _ => Device::Cpu,
    }
}

/// Twenty short synthetic passages, repeated to reach `seq` tokens.
fn batch(seq: usize, n: usize) -> Vec<Vec<u32>> {
    let tok = Tokenizer::from_file(model_dir().join("tokenizer.json")).unwrap();
    let topics = [
        "the harbour at dawn",
        "a bread recipe",
        "tide tables",
        "a bicycle repair",
        "a train timetable",
        "garden soil",
        "a chess opening",
        "winter clothing",
        "a lighthouse keeper",
        "river maps",
        "a tea ceremony",
        "a pottery kiln",
        "a market stall",
        "a sailing knot",
        "a night sky chart",
        "a library card",
        "a violin lesson",
        "a mountain hut",
        "a postage stamp",
        "a weather report",
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
            ids
        })
        .collect()
}

/// Same shape as the trainer: shift, mask via index_select, cross entropy.
/// The first third of each sequence is masked out so the gather path runs.
fn loss_of(model: &LoraQwenModel, cfg: &ModelConfig, ids: &[u32], device: &Device) -> Tensor {
    let n = ids.len();
    let input = Tensor::from_slice(ids, (1, n), device).unwrap();
    let logits = model.forward(&input).unwrap();
    let shifted = logits
        .narrow(1, 0, n - 1)
        .unwrap()
        .reshape((n - 1, cfg.vocab_size))
        .unwrap();
    let targets = Tensor::from_slice(&ids[1..], (n - 1,), device).unwrap();
    let keep: Vec<u32> = ((n / 3) as u32..(n - 1) as u32).collect();
    let keep = Tensor::from_slice(keep.as_slice(), (keep.len(),), device).unwrap();
    candle_nn::loss::cross_entropy(
        &shifted.index_select(&keep, 0).unwrap(),
        &targets.index_select(&keep, 0).unwrap(),
    )
    .unwrap()
}

fn flat(t: &Tensor) -> Vec<f32> {
    t.to_device(&Device::Cpu)
        .unwrap()
        .to_dtype(DType::F32)
        .unwrap()
        .flatten_all()
        .unwrap()
        .to_vec1::<f32>()
        .unwrap()
}

fn norm(v: &[f32]) -> f64 {
    v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt()
}

/// Gates 1: gradient correctness, Metal vs CPU, plus a directional
/// finite-difference check on the Metal gradient.
#[test]
#[ignore]
fn spike_grad_differential() {
    let seq = env_usize("ONDE_SPIKE_SEQ", 64);
    let perturb = env_usize("ONDE_SPIKE_PERTURB", 0) == 1;
    let metal = Device::new_metal(0).expect("metal device");
    let (cpu_model, cfg) = build(&Device::Cpu);
    let (mtl_model, _) = build(&metal);
    let cpu_vars = cpu_model.lora_vars();
    let mtl_vars = mtl_model.lora_vars();

    // Identical LoRA state on both devices.
    for (c, m) in cpu_vars.iter().zip(mtl_vars.iter()) {
        let mut vals = flat(c.as_tensor());
        if perturb && c.as_tensor().dims()[0] != 8 {
            // lora_b is [out, rank]: give it a small deterministic nonzero value.
            for (i, v) in vals.iter_mut().enumerate() {
                *v = (((i * 2654435761) % 1000) as f32 / 1000.0 - 0.5) * 0.02;
            }
        }
        let shape = c.as_tensor().dims().to_vec();
        c.set(&Tensor::from_vec(vals.clone(), shape.clone(), &Device::Cpu).unwrap())
            .unwrap();
        m.set(&Tensor::from_vec(vals, shape, &metal).unwrap())
            .unwrap();
    }

    let ids = &batch(seq, 1)[0];
    let t0 = std::time::Instant::now();
    let lc = loss_of(&cpu_model, &cfg, ids, &Device::Cpu);
    let gc = lc.backward().unwrap();
    let cpu_ms = t0.elapsed().as_millis();
    let t0 = std::time::Instant::now();
    let lm = loss_of(&mtl_model, &cfg, ids, &metal);
    let gm = lm.backward().unwrap();
    let mtl_ms = t0.elapsed().as_millis();
    let (lcv, lmv) = (
        lc.to_scalar::<f32>().unwrap(),
        lm.to_scalar::<f32>().unwrap(),
    );
    eprintln!(
        "seq {seq} perturb {perturb}: loss cpu {lcv:.5} metal {lmv:.5}  fwd+bwd cpu {cpu_ms} ms metal {mtl_ms} ms"
    );

    let mut worst = 0f64;
    let mut worst_name = 0usize;
    let mut bad = 0usize;
    let mut checked = 0usize;
    for (i, (c, m)) in cpu_vars.iter().zip(mtl_vars.iter()).enumerate() {
        let a = gc.get(c.as_tensor()).map(flat);
        let b = gm.get(m.as_tensor()).map(flat);
        let (a, b) = match (a, b) {
            (Some(a), Some(b)) => (a, b),
            (None, None) => continue,
            _ => {
                eprintln!("var {i}: gradient present on one device only");
                bad += 1;
                continue;
            }
        };
        let na = norm(&a);
        if na == 0.0 {
            // lora_a at pristine B=0: gradient is exactly zero; Metal must agree.
            if norm(&b) != 0.0 {
                eprintln!("var {i}: cpu grad is zero, metal norm {:.3e}", norm(&b));
                bad += 1;
            }
            continue;
        }
        let diff: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
        let rel = norm(&diff) / na;
        checked += 1;
        if rel > worst {
            worst = rel;
            worst_name = i;
        }
        if rel > 0.01 {
            bad += 1;
        }
    }
    eprintln!(
        "{checked} nonzero tensors compared, worst relative diff {worst:.3e} (var {worst_name}), {bad} over 1%"
    );

    // Directional finite difference on the Metal gradient of one tensor.
    let target = mtl_vars
        .iter()
        .enumerate()
        .rev()
        .find(|(_, v)| {
            gm.get(v.as_tensor())
                .map(|g| norm(&flat(g)) > 0.0)
                .unwrap_or(false)
        })
        .map(|(i, _)| i)
        .unwrap();
    let v = &mtl_vars[target];
    let g = flat(gm.get(v.as_tensor()).unwrap());
    let gn = norm(&g);
    let dir: Vec<f32> = g.iter().map(|x| (*x as f64 / gn) as f32).collect();
    let base = flat(v.as_tensor());
    let shape = v.as_tensor().dims().to_vec();
    let h = 1e-2f32;
    let eval = |sign: f32| -> f64 {
        let moved: Vec<f32> = base
            .iter()
            .zip(&dir)
            .map(|(b, d)| b + sign * h * d)
            .collect();
        v.set(&Tensor::from_vec(moved, shape.clone(), &metal).unwrap())
            .unwrap();
        loss_of(&mtl_model, &cfg, ids, &metal)
            .to_scalar::<f32>()
            .unwrap() as f64
    };
    let fd = (eval(1.0) - eval(-1.0)) / (2.0 * h as f64);
    v.set(&Tensor::from_vec(base, shape, &metal).unwrap())
        .unwrap();
    eprintln!(
        "finite difference on var {target}: fd {fd:.5e} vs |grad| {gn:.5e} (ratio {:.4})",
        fd / gn
    );

    assert_eq!(bad, 0, "gradient mismatch");
}

/// Gates 2 and 3: overfit a fixed batch and report throughput.
/// `ONDE_SPIKE_DEVICE=cpu|metal`, `ONDE_SPIKE_SEQ`, `ONDE_SPIKE_EXAMPLES`,
/// `ONDE_SPIKE_EPOCHS`, `ONDE_SPIKE_LR`.
#[test]
#[ignore]
fn spike_overfit_bench() {
    let device = device_from_env();
    let seq = env_usize("ONDE_SPIKE_SEQ", 256);
    let n = env_usize("ONDE_SPIKE_EXAMPLES", 20);
    let epochs = env_usize("ONDE_SPIKE_EPOCHS", 3);
    let lr: f64 = std::env::var("ONDE_SPIKE_LR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1e-3);
    let (model, cfg) = build(&device);
    let vars = model.lora_vars();
    let mut opt = AdamW::new(
        vars.clone(),
        ParamsAdamW {
            lr,
            ..ParamsAdamW::default()
        },
    )
    .unwrap();
    let data = batch(seq, n);
    let start = std::time::Instant::now();
    let mut tokens = 0usize;
    for epoch in 0..epochs {
        let mut total = 0f64;
        for ids in &data {
            let loss = loss_of(&model, &cfg, ids, &device);
            let lv = loss.to_scalar::<f32>().unwrap();
            let mut grads = loss.backward().unwrap();
            // Clip like the trainer does.
            let mut sq = 0f64;
            for v in &vars {
                if let Some(g) = grads.get(v.as_tensor()) {
                    let n = norm(&flat(g));
                    sq += n * n;
                }
            }
            let gn = sq.sqrt();
            if gn.is_finite() && gn > 1.0 {
                let coef = 1.0 / (gn + 1e-6);
                for v in &vars {
                    if let Some(g) = grads.get(v.as_tensor()) {
                        let scaled = g.affine(coef, 0.0).unwrap();
                        grads.insert(v.as_tensor(), scaled);
                    }
                }
            }
            opt.step(&grads).unwrap();
            total += lv as f64;
            tokens += ids.len();
        }
        let secs = start.elapsed().as_secs_f64();
        eprintln!(
            "epoch {} mean loss {:.4}  elapsed {:.1}s  {:.0} tok/s",
            epoch + 1,
            total / data.len() as f64,
            secs,
            tokens as f64 / secs
        );
    }
}

// ---------------------------------------------------------------------------
// Op-by-op differential: forward and backward, CPU vs Metal.
// ---------------------------------------------------------------------------

fn rnd(n: usize, seed: u64) -> Vec<f32> {
    let mut s = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..n)
        .map(|_| {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        })
        .collect()
}

fn rel(a: &[f32], b: &[f32]) -> f64 {
    let d: Vec<f32> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    norm(&d) / (norm(a) + 1e-12)
}

/// Run `f` on both devices over identical inputs; report forward and
/// per-input gradient relative differences.
fn check(name: &str, shapes: &[&[usize]], f: &dyn Fn(&[Tensor], &Device) -> Tensor) -> bool {
    let metal = Device::new_metal(0).unwrap();
    let run = |dev: &Device| -> (Vec<f32>, Vec<Vec<f32>>) {
        let vars: Vec<Var> = shapes
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let n: usize = s.iter().product();
                Var::from_tensor(&Tensor::from_vec(rnd(n, 7 + i as u64), s.to_vec(), dev).unwrap())
                    .unwrap()
            })
            .collect();
        let ts: Vec<Tensor> = vars.iter().map(|v| v.as_tensor().clone()).collect();
        let out = f(&ts, dev);
        let w = Tensor::from_vec(rnd(out.elem_count(), 99), out.dims().to_vec(), dev).unwrap();
        let loss = (&out * &w).unwrap().sum_all().unwrap();
        let grads = loss.backward().unwrap();
        let g = vars
            .iter()
            .map(|v| {
                grads
                    .get(v.as_tensor())
                    .map(flat)
                    .unwrap_or_else(|| vec![0.0; v.as_tensor().elem_count()])
            })
            .collect();
        (flat(&out), g)
    };
    let (fc, gc) = run(&Device::Cpu);
    let (fm, gm) = run(&metal);
    let fwd = rel(&fc, &fm);
    let grads: Vec<f64> = gc.iter().zip(&gm).map(|(a, b)| rel(a, b)).collect();
    let ok = fwd < 1e-3 && grads.iter().all(|g| *g < 1e-3);
    eprintln!(
        "{} {name:<28} fwd {fwd:.2e}  grads {}",
        if ok { "ok  " } else { "FAIL" },
        grads
            .iter()
            .map(|g| format!("{g:.2e}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    ok
}

#[test]
#[ignore]
fn spike_op_differential() {
    let mut ok = true;
    macro_rules! c { ($($t:tt)*) => { ok &= check($($t)*); } }
    c!("matmul 2d", &[&[24, 16], &[16, 12]], &|x, _| x[0]
        .matmul(&x[1])
        .unwrap());
    c!("matmul x W^T", &[&[24, 16], &[12, 16]], &|x, _| x[0]
        .matmul(&x[1].t().unwrap())
        .unwrap());
    c!("matmul 4d k^T", &[&[1, 4, 8, 6], &[1, 4, 8, 6]], &|x, _| {
        x[0].matmul(&x[1].transpose(2, 3).unwrap()).unwrap()
    });
    c!("sqr/mean/affine/sqrt/div", &[&[1, 8, 16]], &|x, _| {
        let m = x[0].sqr().unwrap().mean_keepdim(D::Minus1).unwrap();
        x[0].broadcast_div(&m.affine(1.0, 1e-6).unwrap().sqrt().unwrap())
            .unwrap()
    });
    c!("broadcast_mul weight", &[&[1, 8, 16], &[16]], &|x, _| x[0]
        .broadcast_mul(&x[1])
        .unwrap());
    c!("broadcast_add bias", &[&[1, 8, 16], &[16]], &|x, _| x[0]
        .broadcast_add(&x[1])
        .unwrap());
    c!("narrow last dim", &[&[1, 4, 8, 16]], &|x, _| x[0]
        .narrow(D::Minus1, 8, 8)
        .unwrap());
    c!("narrow dim 1", &[&[1, 12, 16]], &|x, _| x[0]
        .narrow(1, 0, 11)
        .unwrap());
    c!("neg+cat last dim", &[&[1, 4, 8, 16]], &|x, _| {
        let a = x[0].narrow(D::Minus1, 0, 8).unwrap();
        let b = x[0].narrow(D::Minus1, 8, 8).unwrap();
        Tensor::cat(&[&b.neg().unwrap(), &a], D::Minus1).unwrap()
    });
    c!("transpose+contiguous", &[&[1, 8, 4, 6]], &|x, _| {
        x[0].transpose(1, 2).unwrap().contiguous().unwrap()
    });
    c!("transpose+reshape", &[&[1, 4, 8, 6]], &|x, _| {
        x[0].transpose(1, 2)
            .unwrap()
            .contiguous()
            .unwrap()
            .reshape((1, 8, 24))
            .unwrap()
    });
    c!("gqa expand+reshape", &[&[1, 2, 8, 6]], &|x, _| {
        x[0].unsqueeze(2)
            .unwrap()
            .expand((1, 2, 4, 8, 6))
            .unwrap()
            .reshape((1, 8, 8, 6))
            .unwrap()
            .contiguous()
            .unwrap()
    });
    c!("expand only (5d)", &[&[1, 2, 1, 8, 6]], &|x, _| x[0]
        .expand((1, 2, 4, 8, 6))
        .unwrap()
        .contiguous()
        .unwrap());
    c!("expand4d only", &[&[1, 1, 8, 6]], &|x, _| x[0]
        .expand((1, 4, 8, 6))
        .unwrap()
        .contiguous()
        .unwrap());
    c!("unsqueeze+expand+contig", &[&[1, 2, 8, 6]], &|x, _| {
        x[0].unsqueeze(2)
            .unwrap()
            .expand((1, 2, 4, 8, 6))
            .unwrap()
            .contiguous()
            .unwrap()
    });
    c!("broadcast_as 3d", &[&[2, 1, 6]], &|x, _| x[0]
        .broadcast_as((2, 4, 6))
        .unwrap()
        .contiguous()
        .unwrap());
    c!("broadcast_as mid-dim", &[&[3, 1, 5, 6]], &|x, _| x[0]
        .broadcast_as((3, 4, 5, 6))
        .unwrap()
        .contiguous()
        .unwrap());
    c!("sum dim 2 (5d)", &[&[1, 2, 4, 8, 6]], &|x, _| x[0]
        .sum_keepdim(2)
        .unwrap());
    c!("sum dim 1 (3d)", &[&[3, 4, 6]], &|x, _| x[0]
        .sum_keepdim(1)
        .unwrap());
    c!("sum dim 0 (3d)", &[&[3, 4, 6]], &|x, _| x[0]
        .sum_keepdim(0)
        .unwrap());
    c!("softmax last dim", &[&[1, 4, 8, 8]], &|x, _| {
        candle_nn::ops::softmax(&x[0], D::Minus1).unwrap()
    });
    c!("causal mask add", &[&[1, 4, 8, 8]], &|x, dev| {
        let tril = Tensor::tril2(8, DType::F32, dev)
            .unwrap()
            .broadcast_as((1, 4, 8, 8))
            .unwrap();
        let bias = (1.0f64 - &tril).unwrap().affine(-1e9, 0.0).unwrap();
        candle_nn::ops::softmax(&(&x[0] + bias).unwrap(), D::Minus1).unwrap()
    });
    c!("silu", &[&[8, 16]], &|x, _| candle_nn::ops::silu(&x[0])
        .unwrap());
    c!("mul+add", &[&[8, 16], &[8, 16]], &|x, _| ((&x[0] * &x[1])
        .unwrap()
        + &x[0])
        .unwrap());
    c!("index_select dim0", &[&[16, 8]], &|x, dev| {
        let idx = Tensor::from_slice(&[3u32, 5, 6, 9, 14, 15], (6,), dev).unwrap();
        x[0].index_select(&idx, 0).unwrap()
    });
    c!("embedding gather (idx dup)", &[&[16, 8]], &|x, dev| {
        let idx = Tensor::from_slice(&[3u32, 3, 5, 3, 9, 5], (6,), dev).unwrap();
        x[0].index_select(&idx, 0).unwrap()
    });
    c!("log_softmax", &[&[12, 32]], &|x, _| {
        candle_nn::ops::log_softmax(&x[0], D::Minus1).unwrap()
    });
    c!("gather dim1 (nll)", &[&[12, 32]], &|x, dev| {
        let idx = Tensor::from_slice(
            &(0..12).map(|i| (i * 5 % 32) as u32).collect::<Vec<_>>(),
            (12, 1),
            dev,
        )
        .unwrap();
        x[0].gather(&idx, 1).unwrap()
    });
    c!("cross_entropy", &[&[12, 32]], &|x, dev| {
        let idx = Tensor::from_slice(
            &(0..12).map(|i| (i * 5 % 32) as u32).collect::<Vec<_>>(),
            (12,),
            dev,
        )
        .unwrap();
        candle_nn::loss::cross_entropy(&x[0], &idx)
            .unwrap()
            .reshape((1,))
            .unwrap()
    });
    c!("select then cross_entropy", &[&[12, 32]], &|x, dev| {
        let keep = Tensor::from_slice(&[2u32, 3, 4, 7, 8, 11], (6,), dev).unwrap();
        let idx = Tensor::from_slice(&[1u32, 9, 4, 30, 2, 17], (6,), dev).unwrap();
        candle_nn::loss::cross_entropy(&x[0].index_select(&keep, 0).unwrap(), &idx)
            .unwrap()
            .reshape((1,))
            .unwrap()
    });
    c!("where_cond", &[&[8, 16], &[8, 16]], &|x, _| {
        let m = x[0].gt(0.0f64).unwrap();
        m.where_cond(&x[0], &x[1]).unwrap()
    });
    assert!(ok, "some ops differ between CPU and Metal");
}

/// Reference for the MLX spike: the perturbed LoRA state, the token ids, the
/// loss and every LoRA gradient, computed on CPU and written to
/// `ONDE_SPIKE_DUMP` (default /tmp/lora_ref.safetensors).
#[test]
#[ignore]
fn spike_dump_reference() {
    let seq = env_usize("ONDE_SPIKE_SEQ", 64);
    let (model, cfg) = build(&Device::Cpu);
    let vars = model.lora_vars();
    for v in vars.iter() {
        let mut vals = flat(v.as_tensor());
        if v.as_tensor().dims()[0] != 8 {
            for (i, x) in vals.iter_mut().enumerate() {
                *x = (((i * 2654435761) % 1000) as f32 / 1000.0 - 0.5) * 0.02;
            }
        }
        let shape = v.as_tensor().dims().to_vec();
        v.set(&Tensor::from_vec(vals, shape, &Device::Cpu).unwrap())
            .unwrap();
    }
    let ids = batch(seq, 1).remove(0);
    let loss = loss_of(&model, &cfg, &ids, &Device::Cpu);
    let grads = loss.backward().unwrap();
    let mut out: HashMap<String, Tensor> = HashMap::new();
    for (i, v) in vars.iter().enumerate() {
        let layer = i / 4;
        let name = match i % 4 {
            0 => "q_proj.lora_a",
            1 => "q_proj.lora_b",
            2 => "v_proj.lora_a",
            _ => "v_proj.lora_b",
        };
        let key = format!("model.layers.{layer}.self_attn.{name}");
        out.insert(key.clone(), v.as_tensor().clone());
        if let Some(g) = grads.get(v.as_tensor()) {
            out.insert(format!("grad.{key}"), g.clone());
        }
    }
    out.insert("loss".into(), loss.reshape((1,)).unwrap());
    let idsf: Vec<f32> = ids.iter().map(|x| *x as f32).collect();
    out.insert(
        "ids".into(),
        Tensor::from_vec(idsf, (ids.len(),), &Device::Cpu).unwrap(),
    );
    let path = std::env::var("ONDE_SPIKE_DUMP").unwrap_or("/tmp/lora_ref.safetensors".into());
    candle_core::safetensors::save(&out, &path).unwrap();
    eprintln!(
        "wrote {} tensors to {path}, loss {}",
        out.len(),
        loss.to_scalar::<f32>().unwrap()
    );
}
