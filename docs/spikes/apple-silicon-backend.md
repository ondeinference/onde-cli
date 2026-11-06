# Apple Silicon training backend: spike results

Date: 2026-11-05. Machine: Apple Silicon Mac, 24 GiB, Xcode 26.6, cmake 4.4.2.
Model: Qwen 3 0.6B, f32, LoRA rank 8 on q_proj and v_proj, one sequence per step.

Three backends were tried for the LoRA trainer in `src/finetune.rs`:

- **CPU** with Accelerate: what the trainer uses today.
- **B, candle Metal**, on `onde-candle` 0.11.2.
- **C, MLX**, through `mlx-rs` 0.34.0 (`docs/spikes/mlx`). Same layer structure and adapter tensor
  names (`model.layers.N.self_attn.{q,v}_proj.lora_{a,b}`) as the candle model.

## Decision

MLX passes all three gates and trains 6.3 times faster than CPU at sequence 256, above the 2x bar.
Use MLX for Apple Silicon. CPU with Accelerate stays everywhere else. candle Metal is out: it is
correct after the fix below, but slower than CPU.

The packaging work in the last section has to be done before MLX becomes the default.

## Gate 1: gradients

Reference: candle CPU, sequence 64, LoRA B set to a small deterministic non-zero value so every
gradient is non-trivial. 112 non-zero LoRA gradient tensors.

| Backend | loss (CPU 3.70209) | worst relative gradient difference | finite difference / analytic |
|---|---|---|---|
| candle Metal, `candle-metal-kernels` 0.11.0 | | 1.29 on the GQA expand and reshape backward | |
| candle Metal, with the fix | 3.96570 vs 3.96569 | 6.7e-5 | 0.9999 |
| MLX | 3.70209 | 4.3e-4 | 0.9982 at 1e-4, 1.0027 at 1e-5 |

With the unperturbed adapter (LoRA B zero), Metal with the fix has a worst difference of 6.9e-5 over
56 tensors and a finite difference ratio of 0.9996. The op-level differential (31 ops, forward and
backward) passes on Metal with the fix, with every difference below 2e-7, including `gqa expand+reshape` at 2e-8.

The finite difference on MLX uses the unit gradient direction of the largest gradient (norm 285).
At a step of 1e-3 the ratio is 0.77 because of curvature, which is the step size, not the gradient.

### The candle Metal bug

`get_strided_idx_fallback` in `reduce.metal` consumed the dimensions after the fourth from the wrong
end, so a strided reduce over a tensor with more than four dimensions read the wrong elements. The
backward of the grouped-query-attention head expand followed by reshape is such a reduce. Upstream
`candle-metal-kernels` 0.11.1 contains the same fix. The regression test is in
`candle-core/tests/tensor_tests.rs` of the `fix/metal-strided-reduce-high-rank` branch of
`setoelkahfi/candle`.

## Gate 2: overfit

Twenty fixed passages, AdamW at 1e-3, gradient norm clipped to 1.0, 6 epochs, sequence 256. Mean loss
per epoch on MLX: 0.390, 2.442, 0.101, 0.036, 0.178, 0.156. The epoch 2 spike is the learning rate on
twenty different passages; the loss is back near 0.1 afterwards. Sequence 1024: 0.540, 0.013, 0.134,
0.021, 0.012, 0.053.

## Gate 3: speed and memory

| Sequence | CPU (candle, Accelerate) | candle Metal | MLX |
|---|---|---|---|
| 64, forward and backward only | 1.4 to 3.0 s | 5.9 s | 0.23 s |
| 64, trainer | | 32 tok/s | |
| 128 | 56 tok/s | did not finish in 590 s | |
| 256 | 105 tok/s, 16.5 GB resident | still running at 18 min | 657 tok/s, 0.97 GB resident, 5.1 GB footprint |
| 1024 | not run | not run | 475 tok/s, 0.82 GB resident, 11.3 GB footprint |

CPU at sequence 1024 was not run: it used 16.5 GB at sequence 256 and this Mac has 24 GiB. MLX at
sequence 256 is 6.3 times CPU. Memory is peak resident set size and peak memory footprint from
`/usr/bin/time -l`. The candle Metal numbers are from the earlier spike session and were not repeated.

## Packaging checks

- **Build:** `mlx-rs` builds MLX from source with cmake. First build 3 minutes 15 seconds. It needs
  cmake and the Xcode Metal toolchain (Xcode 26.6 here; pin it in CI).
- **`cargo install`:** works on the machine that builds. `mlx-sys` writes `mlx.metallib` to
  `~/.mlx/lib/<hash>/` at build time and embeds that absolute path in the binary.
- **A binary built elsewhere fails.** With `~/.mlx/lib/<hash>/` missing the program panics at stream
  creation ("Failed to create stream"). With `mlx.metallib` copied next to the executable it runs. A
  Homebrew bottle, a release tarball or `cargo binstall` artifact must ship `mlx.metallib` beside the
  binary, and that has to be tested on a machine that did not build it.
- **Gate by platform:** the MLX dependency belongs under `cfg(all(target_os = "macos", target_arch = "aarch64"))`,
  on by default there. Not yet tried: a Linux-target `cargo check` with that cfg.

## Reproduce

```bash
# candle side (this crate): reference dump and gates
export ONDE_SPIKE_MODEL_DIR=/path/to/qwen3-0.6b
cargo test --release finetune::spike::spike_dump_reference -- --ignored --nocapture
cargo test --release finetune::spike::spike_op_differential -- --ignored --nocapture
ONDE_SPIKE_PERTURB=1 cargo test --release finetune::spike::spike_grad_differential -- --ignored --nocapture
ONDE_SPIKE_DEVICE=cpu ONDE_SPIKE_SEQ=256 ONDE_SPIKE_EPOCHS=6 cargo test --release finetune::spike::spike_overfit_bench -- --ignored --nocapture

# MLX side
cd docs/spikes/mlx && cargo build --release
./target/release/mlx-spike grad
ONDE_SPIKE_SEQ=256 ONDE_SPIKE_EPOCHS=6 ./target/release/mlx-spike bench
```
