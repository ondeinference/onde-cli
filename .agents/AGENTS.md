# AGENTS.md

`onde-cli` is a Rust TUI (`onde` binary) for managing an Onde Inference account and running a local model pipeline: LoRA fine-tune → adapter merge → GGUF export → local chat test → Hugging Face upload → model assignment to an Onde app. npm, PyPI, NuGet, pub.dev, Homebrew, and crates.io are thin wrappers around this one binary; the Rust crate is the source of truth.

## Build, test, lint

```sh
cargo build                  # debug build
cargo run                    # launch the TUI
cargo run -- --mcp           # run as an MCP server over stdio (no TUI)
cargo test --locked          # unit tests (heavy ones are #[ignore])
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

The build **requires** these env vars (read from `.env` via `build.rs`): `ONDE_APP_ID`, `ONDE_APP_SECRET`, `GRESIQ_API_KEY`, `GRESIQ_API_SECRET`. `HF_TOKEN` is optional. They are baked in at compile time via `env!(...)` in `src/app.rs`. Changing `.env` triggers a full rebuild.

On macOS, candle's `metal` + `accelerate` features are added automatically. **Fine-tuning always runs on CPU** — Metal's backward pass in candle 0.10.2 produces corrupt gradients (1e6–1e29 in magnitude, where CPU gives ~40–65 for identical losses). Use `ONDE_FINETUNE_METAL=1` only to test newer candle versions, and verify with `ONDE_FINETUNE_GRAD_DEBUG=1` (per-step loss/max_abs/norm).

Linux CI requires `libclang-dev` (`sudo apt-get install -y libclang-dev`).

### Tests that load real models

`src/gguf.rs` has two `#[ignore]` integration tests that export a ~1GB GGUF and run inference through `onde::mistralrs`. They need a Qwen3-0.6B model cached in the Onde App Group container and skip gracefully when it is absent.

```sh
cargo test gguf::tests::exported_qwen3_gguf_is_runnable -- --ignored --nocapture
cargo test gguf::tests::finetune_merge_export_run     -- --ignored --nocapture
```

Env knobs for these: `ONDE_TEST_MODEL_DIR`, `ONDE_TEST_DTYPE` (`f16`/`q8_0`), `ONDE_TEST_LR`, `ONDE_TEST_DATA_PATH`, `ONDE_TEST_EPOCHS`, `ONDE_TEST_MAX_SEQ_LEN`, `ONDE_TEST_EVAL_PROMPT` (`||`-separated prompts), `ONDE_TEST_SYSTEM_PROMPT`. This makes the full-pipeline test double as a headless fine-tune runner for a custom dataset.

### Debugging the TUI

`main.rs` redirects both stdout and stderr to `~/.cache/onde/debug.log` before ratatui takes the alternate screen, because `mistral.rs` writes to both fds and would otherwise tear up the display. To see what the app or the inference engine is doing, tail that log; `println!`/`eprintln!`/`log::*` all land there, not on screen.

## Architecture — `src/`

| File(s) | Responsibility |
|---|---|
| `main.rs` | Entry point; redirects stdout/stderr to `~/.cache/onde/debug.log` before ratatui takes over |
| `app.rs` | `App` struct + `Screen` state machine + `AuthEvent` channel (`tokio::select!` over an `mpsc` channel and a `crossterm` `EventStream`); `App::apply` folds background progress into state |
| `ui.rs` | Pure ratatui render of `App`; no SDK imports (re-exports types through `app.rs`) |
| `finetune.rs` | Hand-written LoRA trainer (candle, CPU): Qwen forward pass (RMSNorm, RoPE, GQA, optional Qwen3 QK-norm), LoRA A/B trained on q/v projections in F32, writes `lora_adapter.safetensors`. Gradients are sanitized (non-finite elements zeroed) and globally norm-clipped in a scaled space before each AdamW step; a step is skipped (loudly) only when the norm is zero or non-finite. **Skipping this guard corrupts every weight after the first bad step.** |
| `merge.rs` | Folds LoRA adapter into base weights (`W + scale·(B@A)`); writes `model.safetensors` plus copied config/tokenizer |
| `gguf.rs` | Hand-rolled GGUF writer (no llama.cpp). Tensor dims are written innermost-first — reverse of safetensors shape, because candle reverses on read; `head_dim` comes from config and is emitted as `attention.key_length`/`value_length` (Qwen3 decouples it from `hidden_size/num_heads`); `token_type` is an INT32 array; `general.architecture` is chosen from `model_type` so Qwen3 routes to candle's `quantized_qwen3` loader |
| `chat.rs` | Local GGUF chat via `onde::mistralrs::GgufModelBuilder`, the same engine the Onde SDK uses, so a model can be tested before publishing |
| `hf_upload.rs` / `hf_clone.rs` / `hf_search.rs` / `hf.rs` | Hugging Face Hub: upload the GGUF, check/create a repo, search models, and resolve/merge the local HF cache (incl. the macOS Onde App Group container) |
| `gresiq.rs` | `smbcloud-gresiq-sdk` wrapper for apps and the model catalog. "Deploying" a model means `assign_model(app_id, model_id)` against a catalog entry; the end app then fetches that assignment through the Onde SDK's `load_assigned_model` and downloads the GGUF. The CLI can only assign models that already exist in the GresIQ catalog |
| `token.rs` | Auth token persistence |
| `project.rs` | Per-project fine-tune workspaces under `~/.onde` |

**Background task rule:** network/IO → `tokio::spawn`; CPU-heavy tensor work (`finetune`, `merge`, `gguf`) → `std::thread::spawn` to avoid starving the async runtime.

**Adding a feature pattern:** new `Screen` variant → key handler → `AuthEvent` variant → background task that streams progress events.

### The `onde` dependency

The `onde` crate provides the inference engine (`onde::mistralrs`, a vendored mistral.rs) and `onde::inference::models::SUPPORTED_MODEL_INFO` (the supported-model catalog the inference picker mirrors). It is normally the published crate; `Cargo.toml` has commented `path`/`[patch.crates-io]` blocks for developing against local checkouts of `onde`, the `smbcloud-*` crates, and `candle` — never commit with those uncommented.

## Conventions

- **Git:** merge feature/release/hotfix branches with `--no-ff`. Tag the merge commit on `main`. Never tag a branch tip that hasn't been merged. See `.agents/skills/git/SKILL.md`.
- **Commits:** include `Co-Authored-By: siGit Code <297239231+sigitc@users.noreply.github.com>` trailer when committing with the agent.
- **`[patch.crates-io]` and `path =` deps:** commented out in `Cargo.toml` for local dev against `onde`/`smbcloud-*`/`candle`. Never commit with these uncommented.
- **Distribution:** before touching any wrapper package (npm, PyPI, NuGet, pub.dev, Homebrew), read `.agents/skills/distribution/SKILL.md`. Keep all channel versions aligned with the Rust crate version in `Cargo.toml`.
- **MCP Registry:** the listing's ownership markers ship inside the published npm and NuGet artifacts, so a version published without them can never be listed. See `docs/mcp-registry.md` before touching `server.json` or either marker.
- **Wrapper packages** are in `npm/`, `pypi/`, `nuget/`, `pub/`. The npm base manifest is rewritten at release time by `npm/scripts/render-main-package.cjs`; do not hand-edit committed package.json versions as the source of truth for releases.
- **Clippy** runs with `-D warnings` in CI; all lints must pass.
- **Rust edition:** 2024; toolchain: stable.
