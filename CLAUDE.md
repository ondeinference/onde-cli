# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`onde-cli` is a native Rust terminal UI (binary name `onde`) for managing an Onde Inference account and running a local model pipeline: fine-tune a safetensors small language model with LoRA, merge the adapter, export to GGUF, test it in a local chat, upload it to Hugging Face, and assign it to an Onde app. All packaging (npm, PyPI, NuGet, pub.dev, Homebrew, crates.io) is thin wrappers around this one binary; the Rust crate is the source of truth.

## Build, run, test

```sh
cargo build                 # debug build
cargo run                   # launches the TUI
cargo test                  # unit tests only (the heavy ones are #[ignore])
cargo clippy --all-targets
```

The build **bakes credentials in at compile time** via `build.rs`, which reads `.env` (or the environment in CI). The build fails if any of these are missing: `ONDE_APP_ID`, `ONDE_APP_SECRET`, `GRESIQ_API_KEY`, `GRESIQ_API_SECRET`. `HF_TOKEN` is optional (defaults to empty). Changing `.env` triggers a rebuild. These are exposed in code as `env!(...)` constants in `src/app.rs`.

On macOS the build pulls in candle's `metal` and `accelerate` features, so inference and training run on Metal; elsewhere they fall back to CPU.

### Tests that load real models

`src/gguf.rs` has two `#[ignore]` integration tests that export a ~1GB GGUF and run inference through `onde::mistralrs`. They need a Qwen3-0.6B model cached in the Onde App Group container and skip gracefully when it is absent.

```sh
cargo test gguf::tests::exported_qwen3_gguf_is_runnable -- --ignored --nocapture
cargo test gguf::tests::finetune_merge_export_run     -- --ignored --nocapture
```

Env knobs for these: `ONDE_TEST_MODEL_DIR`, `ONDE_TEST_DTYPE` (`f16`/`q8_0`), `ONDE_TEST_LR`.

### Debugging the TUI

`main.rs` redirects both stdout and stderr to `~/.cache/onde/debug.log` before ratatui takes the alternate screen, because `mistral.rs` writes to both fds and would otherwise tear up the TUI. To see what the app or the inference engine is doing, tail that log; `println!`/`eprintln!`/`log::*` all land there, not on screen.

## Architecture

### TUI event loop (`app.rs`, `ui.rs`, `main.rs`)

The whole app is one `App` struct plus a `Screen` enum acting as a state machine. `app::run` owns a single `tokio::sync::mpsc` channel of `AuthEvent`s and a `crossterm` `EventStream`, multiplexed with `tokio::select!`. Keystrokes mutate `App` synchronously; anything slow (network calls, downloads, fine-tune, merge, GGUF export, chat inference) is spawned as a background tokio task or OS thread that streams progress back as `AuthEvent`s, which `App::apply` folds into state. `ui.rs` is a pure render of `App` and intentionally does not depend on the SDK directly (it re-exports `OndeApp`/`OndeModel` through `app.rs`). When adding a feature, the pattern is: add a `Screen` variant, a key handler, an `AuthEvent` variant, and a background task that emits progress.

Background work uses two task kinds deliberately: network/IO uses `tokio::spawn`; CPU-heavy tensor work (`finetune`, `merge`, `gguf`) uses `std::thread::spawn` so it does not starve the async runtime.

### The local model pipeline

These modules form the fine-tune-to-deploy chain, each running on a background thread and streaming a `*Progress` enum:

- `finetune.rs` — hand-written LoRA trainer (candle). Builds a Qwen forward pass (RMSNorm, RoPE, GQA, optional Qwen3 QK-norm), trains LoRA A/B on q/v projections in F32, writes `lora_adapter.safetensors`. Gradients are sanitized (non-finite elements zeroed) and globally norm-clipped before each AdamW step; a step is skipped entirely if its global grad norm is non-finite. Skipping this guard corrupts every weight after the first bad step.
- `merge.rs` — folds the LoRA adapter back into base weights (`W + scale·(B@A)`), writes a merged `model.safetensors` plus copied config/tokenizer.
- `gguf.rs` — **hand-rolled GGUF writer** (no llama.cpp). Conventions that must hold for mistral.rs/candle to load and run the file correctly: tensor dims are written innermost-first (reverse of safetensors shape, because candle reverses on read); `head_dim` comes from config and is emitted as `attention.key_length`/`value_length` (Qwen3 decouples it from `hidden_size/num_heads`); `token_type` is an INT32 array; `general.architecture` is chosen from `model_type` so Qwen3 routes to candle's `quantized_qwen3` loader.
- `chat.rs` — loads a local GGUF via `onde::mistralrs::GgufModelBuilder` (the same engine the Onde SDK uses) and streams a multi-turn chat, so a model can be tested before publishing.
- `hf_upload.rs` / `hf_clone.rs` / `hf_search.rs` / `hf.rs` — Hugging Face Hub: upload the GGUF, check/create a repo, search models, and resolve/merge the local HF cache (incl. the macOS Onde App Group container).

### Account / deploy side (`gresiq.rs`, `token.rs`, `project.rs`)

`gresiq.rs` wraps `smbcloud-gresiq-sdk` for apps and the model catalog. "Deploying" a model means `assign_model(app_id, model_id)` against a catalog entry; the end app (e.g. `rumilearnpersian`) then fetches that assignment through the Onde SDK's `load_assigned_model` and downloads the GGUF. The CLI can only assign models that already exist in the GresIQ catalog. `token.rs` persists the auth token; `project.rs` manages per-project fine-tune workspaces under `~/.onde`.

### The `onde` dependency

The `onde` crate provides the inference engine (`onde::mistralrs`, a vendored mistral.rs) and `onde::inference::models::SUPPORTED_MODEL_INFO` (the supported-model catalog the inference picker mirrors). It is normally the published crate; `Cargo.toml` has commented `path`/`[patch.crates-io]` blocks for developing against local checkouts of `onde`, the `smbcloud-*` crates, and `candle`. Never commit with those uncommented.

## Conventions

- Git: merge feature/release/hotfix branches with `--no-ff` (explicit merge commits). Tag the merge commit that holds the final release state. See `.agents/skills/git/SKILL.md`.
- Distribution changes: see `.agents/skills/distribution/SKILL.md` before touching any wrapper package; keep all channel versions aligned with the Rust crate version.
