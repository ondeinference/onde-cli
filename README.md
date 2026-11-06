<p align="center">
  <img src="docs/assets/logo.svg" alt="Onde Inference" width="96">
</p>

<h1 align="center">Onde Inference CLI</h1>

<p align="center">
  <strong>Fine-tune, export GGUF and manage Onde from the terminal and from agents.</strong>
</p>

<p align="center">
  <a href="https://crates.io/crates/onde-cli"><img src="https://img.shields.io/crates/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/@ondeinference/cli"><img src="https://img.shields.io/npm/v/@ondeinference/cli?style=flat-square&labelColor=17211D&color=235843" alt="npm"></a>
  <a href="https://pypi.org/project/onde-cli/"><img src="https://img.shields.io/pypi/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="PyPI"></a>
  <a href="https://pub.dev/packages/onde_cli"><img src="https://img.shields.io/pub/v/onde_cli?style=flat-square&labelColor=17211D&color=235843" alt="pub.dev"></a>
  <a href="https://www.nuget.org/packages/Onde.Cli"><img src="https://img.shields.io/nuget/v/Onde.Cli?style=flat-square&labelColor=17211D&color=235843" alt="NuGet"></a>
  <a href="https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.ondeinference/onde-cli"><img src="https://img.shields.io/badge/MCP%20Registry-listed-235843?style=flat-square&labelColor=17211D" alt="MCP Registry"></a>
  <a href="https://github.com/ondeinference/onde-cli/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/ondeinference/onde-cli/ci.yml?branch=main&style=flat-square&labelColor=17211D&color=235843&label=CI" alt="CI"></a>
  <a href="#license"><img src="https://img.shields.io/crates/l/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="License"></a>
</p>

`onde` is the command-line client for [Onde Inference](https://ondeinference.com/). It manages your
account, apps and model catalog, and it fine-tunes Qwen models locally with LoRA and exports them to
GGUF. It runs as a terminal UI, or as an MCP server for agents.

## Install

Pick one:

```sh
# Homebrew
brew tap ondeinference/homebrew-tap && brew trust --tap ondeinference/homebrew-tap && brew install onde
```

```sh
# npm
npm install -g @ondeinference/cli
```

```sh
# Cargo
cargo install onde-cli
```

More ways, including pip/uv, .NET, Dart pub, winget, Scoop and pre-built binaries, are in
[docs/install.md](docs/install.md).

## Use it

- `onde --tui` opens the terminal UI where you sign in and manage apps and models ([docs/usage.md](docs/usage.md)).
- `onde --mcp` runs an MCP server over stdio for any MCP client ([docs/mcp.md](docs/mcp.md)).
- Fine-tune a model with LoRA and export it to GGUF ([docs/finetune.md](docs/finetune.md)).

## Docs

[Install](docs/install.md) · [Usage](docs/usage.md) · [MCP server](docs/mcp.md) · [MCP Registry](docs/mcp-registry.md) · [Fine-tuning](docs/finetune.md)

## License

Dual-licensed under [MIT](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-MIT) and [Apache 2.0](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-APACHE).

## Copyright

© 2026 [Splitfire AB](https://5mb.app) ([Onde Inference](https://ondeinference.com)).
