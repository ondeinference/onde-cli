<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde/main/assets/onde-inference-logo.svg" alt="Onde Inference" width="80">
</p>

<h1 align="center">onde</h1>

<p align="center">
  <strong>A terminal app for managing Onde Inference apps and fine-tuning models locally.</strong><br>
  Sign in, assign models, and train adapters without bouncing between browser tabs.
</p>

<p align="center">
  <a href="https://ondeinference.com"><img src="https://img.shields.io/badge/ondeinference.com-235843?style=flat-square&labelColor=17211D" alt="Website"></a>
  <a href="https://pypi.org/project/onde-cli/"><img src="https://img.shields.io/pypi/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="PyPI"></a>
  <a href="https://www.npmjs.com/package/@ondeinference/cli"><img src="https://img.shields.io/npm/v/@ondeinference/cli?style=flat-square&labelColor=17211D&color=235843" alt="npm"></a>
  <a href="https://crates.io/crates/onde-cli"><img src="https://img.shields.io/crates/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="Crates.io"></a>
  <a href="https://github.com/ondeinference/onde-cli/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-235843?style=flat-square&labelColor=17211D" alt="License"></a>
</p>

<br>

<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde-cli/main/assets/screenshot.png" alt="onde CLI app list and model assignments" width="720">
</p>

<br>

---

## Install

```sh
pip install onde-cli
```

That installs the native `onde` binary for your platform. No compiler setup, no Node.js, no extra runtime to babysit.

## What you get

Run:

```sh
onde
```

and you'll get a full terminal UI where you can:

- sign up or sign in
- create and rename apps
- assign models to apps
- fine-tune supported Qwen models locally
- merge adapters and export GGUF files

If you prefer doing this work in a terminal instead of a browser, that's the whole point.

## Keyboard shortcuts

### Auth screen

| Key | Action |
|---|---|
| `Tab` | Move between fields |
| `Enter` | Submit |
| `Ctrl+L` | Switch to sign in |
| `Ctrl+N` | Switch to create account |
| `Ctrl+C` | Quit |

### Apps list

| Key | Action |
|---|---|
| `↑` `↓` | Move through apps |
| `Enter` | Open app details |
| `n` | Create a new app |
| `s` | Sign out |
| `Ctrl+C` | Quit |

### App detail

| Key | Action |
|---|---|
| `m` | Assign or change model |
| `r` | Rename app |
| `s` | Sign out |
| `Esc` | Back to apps list |

### Model picker

| Key | Action |
|---|---|
| `↑` `↓` | Move through models |
| `Enter` | Assign selected model |
| `Esc` | Cancel |

### Fine-tuning

| Key | Action |
|---|---|
| `↑` `↓` | Move through models |
| `f` | Open fine-tune config for the selected model |
| `m` | Merge adapter into base model |
| `g` | Export merged model to GGUF |

## Fine-tuning

`onde` can fine-tune Qwen2, Qwen2.5, and Qwen3 safetensors models with LoRA.

Training runs locally:
- on Metal for Apple Silicon
- on CPU everywhere else

No cloud training job. No Python environment. No notebook setup spiral.

### Supported base models

| Model | Size |
|---|---|
| `Qwen/Qwen3-0.6B` | ~1.2 GB |
| `Qwen/Qwen2.5-1.5B-Instruct` | ~3.0 GB |
| `Qwen/Qwen3-1.7B` | ~3.4 GB |
| `Qwen/Qwen3-4B` | ~8.0 GB |

Only safetensors models can be fine-tuned. GGUF models are already quantized, so this pipeline doesn't use them as training inputs.

### Training data format

Use one JSON object per line. Each object needs a `text` field containing the full conversation in Qwen's chat template.

```jsonl
{"text": "<|im_start|>user\nWhat is the boiling point of water?<|im_end|>\n<|im_start|>assistant\n100°C at sea level.<|im_end|>"}
{"text": "<|im_start|>user\nWhat about at high altitude?<|im_end|>\n<|im_start|>assistant\nLower, around 90°C at 3000 m.<|im_end|>"}
```

### Running a fine-tune

1. Open the Models tab.
2. Pick a safetensors model with `↑` / `↓`.
3. Press `f`.
4. Set your training data path, LoRA rank, epochs, and learning rate.
5. Start training.

Default values:
- LoRA rank: `8`
- Epochs: `3`
- Learning rate: `0.0001`

As a rough reference, a rank-8 adapter for the 0.6B model is about 1.5 MB.

### After training

Once the run finishes:

- press `m` to merge the adapter into the base weights
- press `g` to export the merged model to GGUF

The exported file loads directly in the [Onde SDK](https://ondeinference.com) for on-device inference.

## Other ways to install

| Method | Command |
|---|---|
| npm | `npm install -g @ondeinference/cli` |
| Homebrew | `brew install ondeinference/homebrew-tap/onde` |
| uv | `uv tool install onde-cli` |
| Cargo | `cargo install onde-cli` |

### Build from source

```sh
git clone https://github.com/ondeinference/onde-cli
cd onde-cli
cargo build --release
./target/release/onde
```

## Platform support

Prebuilt native binaries are available for:

| Platform | Architecture |
|---|---|
| macOS | arm64, x64 |
| Linux (glibc) | arm64, x64 |
| Windows | arm64, x64 |

## Debug logs

Logs are written to `~/.cache/onde/debug.log`.

The app uses the terminal screen directly while the TUI is open, so logs go to the file instead of printing over the interface.

## Related SDKs

| SDK | Install |
|---|---|
| [Swift SDK](https://github.com/ondeinference/onde-swift) | Swift Package Manager |
| [Flutter SDK](https://pub.dev/packages/onde_inference) | `flutter pub add onde_inference` |
| [React Native SDK](https://www.npmjs.com/package/@ondeinference/react-native) | `npm i @ondeinference/react-native` |
| [Rust crate](https://crates.io/crates/onde) | `cargo add onde` |

## Source and issues

This package ships a prebuilt native binary. The source code lives here:

[github.com/ondeinference/onde-cli](https://github.com/ondeinference/onde-cli)

Bug reports and feature requests should go there too.

## License

Dual-licensed under **MIT** and **Apache 2.0**.

- [MIT License](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-MIT)
- [Apache License 2.0](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-APACHE)

---

<p align="center">
  <sub>Built by <a href="https://ondeinference.com">Onde Inference</a> · © 2026</sub>
</p>