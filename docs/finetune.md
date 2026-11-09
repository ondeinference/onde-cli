# Fine-tuning

`onde` includes a LoRA fine-tuning pipeline for Qwen2, Qwen2.5, and Qwen3 models. It runs locally: Metal on Apple Silicon, CPU elsewhere. No cloud setup. No Python environment.

The flow is straightforward: download a safetensors base model, fine-tune it with LoRA, merge the adapter back into the base weights, then export to GGUF for use in the Onde SDK.

If you want a quick refresher on what the model is actually doing at inference time, Onde has a short note on the [forward pass](https://ondeinference.com/forward-pass).

## Training data format

Each line should be one complete conversation in Qwen's chat template:

```jsonl
{"text": "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\nWhat is LoRA?<|im_end|>\n<|im_start|>assistant\nLoRA adds small trainable matrices to frozen layers, letting you fine-tune large models without updating all the weights.<|im_end|>"}
```

Save the file wherever you want. The TUI lets you point to it directly.

## Running it

```
onde --tui
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

## After training

For rank 8 on a 0.6B model, the adapter is about 1.5 MB. From the fine-tune complete screen:

- `m` to merge the adapter into the base model
- `g` to export the merged model to GGUF as Q8_0
- `G` (Shift+g) to export it as Q4_K_M, about a quarter the size of F16

The resulting GGUF loads directly in the [Onde SDK](https://ondeinference.com/sdk) for on-device AI inference.

Q4_K_M follows llama.cpp's mix: Q4_K for most weights, Q6_K for `attn_v` and `ffn_down` in the first and last eighth of the layers and every third layer between, and Q6_K for the output head (the token embedding when the model ties them, in which case no separate `output.weight` is written). Norms and biases stay F32. Tensors whose row length is not a multiple of 256 fall back to Q8_0.

## Supported base models

| Model | Size | Notes |
|---|---|---|
| `Qwen/Qwen3-0.6B` | ~1.2 GB | Smallest and quickest to train |
| `Qwen/Qwen2.5-1.5B-Instruct` | ~3.0 GB | Good default for instruction tuning |
| `Qwen/Qwen3-1.7B` | ~3.4 GB | Newer small Qwen3 model |
| `Qwen/Qwen3-4B` | ~8.0 GB | Best quality, better suited to macOS |

You can search for any of these from the Models tab with `/`.
