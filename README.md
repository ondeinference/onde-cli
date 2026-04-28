<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde/main/assets/onde-inference-logo.svg" alt="Onde Inference" width="96">
</p>

<h1 align="center">onde</h1>

<p align="center">
  Command-line interface for <a href="https://ondeinference.com">Onde Inference</a>.
</p>

<p align="center">
  <a href="https://ondeinference.com"><img src="https://img.shields.io/badge/ondeinference.com-235843?style=flat-square&labelColor=17211D" alt="Website"></a>
  <a href="https://apps.apple.com/se/developer/splitfire-ab/id1831430993"><img src="https://img.shields.io/badge/App%20Store-live-235843?style=flat-square&labelColor=17211D" alt="App Store"></a>
  <a href="https://www.npmjs.com/package/@ondeinference/cli"><img src="https://img.shields.io/npm/v/@ondeinference/cli?style=flat-square&labelColor=17211D&color=235843" alt="npm"></a>
  <a href="https://pypi.org/project/onde-cli/"><img src="https://img.shields.io/pypi/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="PyPI"></a>
  <a href="https://crates.io/crates/onde-cli"><img src="https://img.shields.io/crates/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="Crates.io"></a>
</p>

<p align="center">
  <a href="https://github.com/ondeinference/onde-swift">Swift</a> · <a href="https://pub.dev/packages/onde_inference">Flutter</a> · <a href="https://www.npmjs.com/package/@ondeinference/react-native">React Native</a> · <a href="https://crates.io/crates/onde">Rust</a> · <a href="https://ondeinference.com">Website</a>
</p>

---

Manage your Onde Inference account, fine-tune local models, and export them to GGUF, all from the terminal.

## Install

### npm

```sh
npm install -g @ondeinference/cli
```

### Homebrew

```sh
brew tap ondeinference/homebrew-tap
brew install onde
```

### pip / uv

```sh
pip install onde-cli
# or
uv tool install onde-cli
```

### Pre-built binary

Download a release from [GitHub Releases](https://github.com/ondeinference/onde-cli/releases):

```sh
# macOS Apple Silicon
curl -Lo onde https://github.com/ondeinference/onde-cli/releases/latest/download/onde-macos-arm64
chmod +x onde && mv onde /usr/local/bin/onde
```

| Platform | File |
|---|---|
| macOS Apple Silicon | `onde-macos-arm64` |
| macOS Intel | `onde-macos-amd64` |
| Linux x64 | `onde-linux-amd64` |
| Linux arm64 | `onde-linux-arm64` |
| Windows x64 | `onde-win-amd64.exe` |
| Windows arm64 | `onde-win-arm64.exe` |

### cargo

```sh
cargo install onde-cli
```

### Build from source

```sh
git clone https://github.com/ondeinference/onde-cli
cd onde-cli
cargo build --release
./target/release/onde
```

---

## Usage

```sh
onde
```

This opens the TUI. You can sign up or sign in right there.

| Key | What it does |
|---|---|
| `Tab` | Move between fields |
| `Enter` | Submit or sign out |
| `Ctrl+L` | Go to the sign-in screen |
| `Ctrl+N` | Go to the new account screen |
| `Ctrl+C` | Quit |

---

## Fine-tuning

`onde` includes a LoRA fine-tuning pipeline for Qwen2, Qwen2.5, and Qwen3 models. It runs locally: Metal on Apple Silicon, CPU elsewhere. No cloud setup. No Python environment.

The flow is straightforward: download a safetensors base model, fine-tune it with LoRA, merge the adapter back into the base weights, then export to GGUF for use in the Onde SDK.

### Training data format

Each line should be one complete conversation in Qwen's chat template:

```jsonl
{"text": "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\nWhat is LoRA?<|im_end|>\n<|im_start|>assistant\nLoRA adds small trainable matrices to frozen layers, letting you fine-tune large models without updating all the weights.<|im_end|>"}
```

Save the file wherever you want. The TUI lets you point to it directly.

### Running it

```
onde
  → Models tab (Tab from Apps)
  → Select a safetensors model (↑↓, Enter)
  → Press f
```

Only safetensors models can be fine-tuned. GGUF models are already quantized, so their weights are not differentiable.

Configure the run:

| Field | Default | Notes |
|---|---|---|
| Training data | `~/.onde/finetune/train.jsonl` | Path to your JSONL file |
| LoRA rank | `8` | Higher means more capacity and more memory use |
| Epochs | `3` | Full passes over the dataset |
| Learning rate | `0.0001` | AdamW default |

Press `Enter` to start. In a healthy run, loss usually starts dropping by epoch 2. If it stays flat, try `0.0003`.

### After training

For rank 8 on a 0.6B model, the adapter is about 1.5 MB. From the fine-tune complete screen:

- `m` to merge the adapter into the base model
- `g` to export the merged model to GGUF

The resulting GGUF loads directly in the Onde SDK for on-device inference.

### Supported base models

| Model | Size | Notes |
|---|---|---|
| `Qwen/Qwen3-0.6B` | ~1.2 GB | Smallest and quickest to train |
| `Qwen/Qwen2.5-1.5B-Instruct` | ~3.0 GB | Good default for instruction tuning |
| `Qwen/Qwen3-1.7B` | ~3.4 GB | Newer small Qwen3 model |
| `Qwen/Qwen3-4B` | ~8.0 GB | Best quality, better suited to macOS |

You can search for any of these from the Models tab with `/`.

---

## Debug

Logs are written to `~/.cache/onde/debug.log`.

---

## License

Dual-licensed under [MIT](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-MIT) and [Apache 2.0](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-APACHE).

<p align="center">
  <sub>© 2026 <a href="https://ondeinference.com">Onde Inference</a> · <a href="https://apps.apple.com/se/developer/splitfire-ab/id1831430993">Splitfire AB</a></sub>
</p>