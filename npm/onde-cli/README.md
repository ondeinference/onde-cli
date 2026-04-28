<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde/main/assets/onde-inference-logo.svg" alt="Onde Inference" width="80">
</p>

<h1 align="center">@ondeinference/cli</h1>

<p align="center">
  A terminal app for your Onde Inference account, plus local model fine-tuning.
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@ondeinference/cli"><img src="https://img.shields.io/npm/v/@ondeinference/cli?style=flat-square&labelColor=17211D&color=235843" alt="npm"></a>
  <a href="https://crates.io/crates/onde-cli"><img src="https://img.shields.io/crates/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="crates.io"></a>
  <a href="https://pypi.org/project/onde-cli/"><img src="https://img.shields.io/pypi/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="PyPI"></a>
  <a href="https://ondeinference.com"><img src="https://img.shields.io/badge/ondeinference.com-235843?style=flat-square&labelColor=17211D" alt="Website"></a>
</p>

---

## Install

```sh
npm install -g @ondeinference/cli
```

npm installs the right native binary for your platform automatically.

It works on:

- macOS (Apple Silicon and Intel)
- Linux (x64 and arm64)
- Windows (x64 and arm64)

### Other ways to install

| Method | Command |
|---|---|
| Homebrew | `brew install ondeinference/homebrew-tap/onde` |
| pip | `pip install onde-cli` |
| uv | `uv tool install onde-cli` |
| Cargo | `cargo install onde-cli` |

---

## Run it

```sh
onde
```

That opens the terminal UI.

From there you can:

- sign up or sign in
- create and manage apps
- assign models
- fine-tune supported local models
- export merged models to GGUF

No browser needed.

## Basic keys

| Key | Action |
|---|---|
| `Tab` | Move between fields |
| `Enter` | Submit or confirm |
| `Ctrl+L` | Go to sign in |
| `Ctrl+N` | Go to create account |
| `Ctrl+C` | Quit |

---

## Fine-tuning

`onde` can fine-tune Qwen2, Qwen2.5, and Qwen3 safetensors models with LoRA.

Training runs locally:
- Metal on Apple Silicon
- CPU on other platforms

So yes, no cloud training setup and no Python environment to babysit.

### Supported base models

| Model | Size |
|---|---|
| `Qwen/Qwen3-0.6B` | ~1.2 GB |
| `Qwen/Qwen2.5-1.5B-Instruct` | ~3.0 GB |
| `Qwen/Qwen3-1.7B` | ~3.4 GB |
| `Qwen/Qwen3-4B` | ~8.0 GB |

Only safetensors models can be fine-tuned. GGUF models are already quantized, so their weights are not differentiable.

### Training data

Use one JSON object per line. Each object needs a `text` field containing the full conversation in Qwen's chat template.

```jsonl
{"text": "<|im_start|>user\nWhat is the boiling point of water?<|im_end|>\n<|im_start|>assistant\n100°C at sea level.<|im_end|>"}
```

### Running a fine-tune

1. Open the Models tab.
2. Pick a safetensors model with `↑` / `↓`.
3. Press `f` to open the fine-tune config.
4. Set your data path, LoRA rank (default `8`), epochs (default `3`), and learning rate (default `0.0001`).
5. Start training.

A rank-8 adapter for the 0.6B model is about 1.5 MB, so the output stays pretty small.

### After training

- Press `m` to merge the adapter into the base weights.
- Press `g` to export the merged model to GGUF.

The exported file loads directly in the [Onde SDK](https://ondeinference.com) for on-device inference.

---

## What is Onde?

[Onde Inference](https://ondeinference.com) is for running LLMs on the user's device. No server round-trips, no sending prompts off to somebody else's machine.

It ships as native SDKs for:

<p align="center">
  <a href="https://github.com/ondeinference/onde-swift">Swift</a>&ensp;·&ensp;<a href="https://pub.dev/packages/onde_inference">Flutter</a>&ensp;·&ensp;<a href="https://www.npmjs.com/package/@ondeinference/react-native">React Native</a>&ensp;·&ensp;<a href="https://crates.io/crates/onde">Rust</a>
</p>

The CLI is for account management and local fine-tuning. The SDKs are what you ship in your app.

---

## Debug

Logs are written to `~/.cache/onde/debug.log`.

## License

Dual-licensed under [MIT](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-MIT) and [Apache 2.0](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-APACHE).

<p align="center">
  <sub>© 2026 <a href="https://ondeinference.com">Onde Inference</a> · <a href="https://apps.apple.com/se/developer/splitfire-ab/id1831430993">Splitfire AB</a></sub>
</p>