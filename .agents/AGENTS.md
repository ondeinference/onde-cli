# AGENTS.md

`onde-cli` is a Rust TUI (`onde` binary) for managing an Onde Inference account and running a local model pipeline: LoRA fine-tune → adapter merge → GGUF export → local chat test → Hugging Face upload → model assignment to an Onde app. npm, PyPI, NuGet, pub.dev, Homebrew, and crates.io are thin wrappers around this one binary; the Rust crate is the source of truth.

## Build, test, lint

```sh
cargo build                  # debug build
cargo run                    # launch the TUI
cargo test --locked          # unit tests (heavy ones are #[ignore])
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

The build **requires** these env vars (read from `.env` via `build.rs`): `ONDE_APP_ID`, `ONDE_APP_SECRET`, `GRESIQ_API_KEY`, `GRESIQ_API_SECRET`. `HF_TOKEN` is optional. They are baked in at compile time via `env!(...)` in `src/app.rs`. Changing `.env` triggers a full rebuild.

On macOS, candle's `metal` + `accelerate` features are added automatically. **Fine-tuning always runs on CPU** — Metal's backward pass in candle 0.10.2 produces corrupt gradients. Use `ONDE_FINETUNE_METAL=1` only to test newer candle versions, and verify with `ONDE_FINETUNE_GRAD_DEBUG=1`.

Linux CI requires `libclang-dev` (`sudo apt-get install -y libclang-dev`).

## Architecture — `src/`

| File(s) | Responsibility |
|---|---|
| `main.rs` | Entry point; redirects stdout/stderr to `~/.cache/onde/debug.log` before ratatui takes over |
| `app.rs` | `App` struct + `Screen` state machine + `AuthEvent` channel; `App::apply` folds background progress into state |
| `ui.rs` | Pure ratatui render of `App`; no SDK imports (re-exports types through `app.rs`) |
| `finetune.rs` | Hand-written LoRA trainer (candle, CPU). Writes `lora_adapter.safetensors` |
| `merge.rs` | Folds LoRA adapter into base weights; writes `model.safetensors` |
| `gguf.rs` | Hand-rolled GGUF writer (no llama.cpp). Tensor dims innermost-first; `head_dim` → `attention.key_length/value_length` |
| `chat.rs` | Local GGUF chat via `onde::mistralrs::GgufModelBuilder` |
| `hf*.rs` | Hugging Face Hub: upload, clone, search, cache resolution |
| `gresiq.rs` | `smbcloud-gresiq-sdk` wrapper; model assignment (`assign_model`) |
| `token.rs` | Auth token persistence |
| `project.rs` | Per-project fine-tune workspaces under `~/.onde` |

**Background task rule:** network/IO → `tokio::spawn`; CPU-heavy tensor work (`finetune`, `merge`, `gguf`) → `std::thread::spawn` to avoid starving the async runtime.

**Adding a feature pattern:** new `Screen` variant → key handler → `AuthEvent` variant → background task that streams progress events.

## Conventions

- **Git:** merge feature/release/hotfix branches with `--no-ff`. Tag the merge commit on `main`. Never tag a branch tip that hasn't been merged. See `.agents/skills/git/SKILL.md`.
- **Commits:** include `Co-Authored-By: siGit Code <297239231+sigitc@users.noreply.github.com>` trailer when committing with the agent.
- **`[patch.crates-io]` and `path =` deps:** commented out in `Cargo.toml` for local dev against `onde`/`smbcloud-*`/`candle`. Never commit with these uncommented.
- **Distribution:** before touching any wrapper package (npm, PyPI, NuGet, pub.dev, Homebrew), read `.agents/skills/distribution/SKILL.md`. Keep all channel versions aligned with the Rust crate version in `Cargo.toml`.
- **Wrapper packages** are in `npm/`, `pypi/`, `nuget/`, `pub/`. The npm base manifest is rewritten at release time by `npm/scripts/render-main-package.cjs`; do not hand-edit committed package.json versions as the source of truth for releases.
- **Clippy** runs with `-D warnings` in CI; all lints must pass.
- **Rust edition:** 2024; toolchain: stable.
