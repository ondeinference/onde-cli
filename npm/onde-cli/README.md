<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde/main/assets/onde-inference-logo.svg" alt="Onde Inference" width="80">
</p>

<h1 align="center">@ondeinference/cli</h1>

<p align="center">
  Manage your Onde Inference account from the terminal.
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

The right binary for your platform gets pulled in automatically. Works on macOS (Apple Silicon and Intel), Linux (x64 and arm64), and Windows (x64 and arm64).

### Other ways to install

| Method | Command |
|---|---|
| **Homebrew** | `brew install ondeinference/homebrew-tap/onde` |
| **pip** | `pip install onde-cli` |
| **uv** | `uv tool install onde-cli` |
| **Cargo** | `cargo install onde-cli` |

---

## Usage

```sh
onde
```

Opens a TUI. Sign up or sign in from there, no browser needed.

| Key | What it does |
|---|---|
| `Tab` | Move between fields |
| `Enter` | Submit / sign out |
| `Ctrl+L` | Sign in screen |
| `Ctrl+N` | New account screen |
| `Ctrl+C` | Quit |

---

## What's Onde?

[Onde Inference](https://ondeinference.com) runs LLMs on the user's device. No server round-trips, no data leaving the hardware. It ships as a native SDK for each platform:

<p align="center">
  <a href="https://github.com/ondeinference/onde-swift">Swift</a>&ensp;·&ensp;<a href="https://pub.dev/packages/onde_inference">Flutter</a>&ensp;·&ensp;<a href="https://www.npmjs.com/package/@ondeinference/react-native">React Native</a>&ensp;·&ensp;<a href="https://crates.io/crates/onde">Rust</a>
</p>

This CLI handles the account side of things. The SDKs handle inference.

---

## Debug

Logs go to `~/.cache/onde/debug.log`.

## License

Dual-licensed under [MIT](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-MIT) and [Apache 2.0](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-APACHE).

<p align="center">
  <sub>© 2026 <a href="https://ondeinference.com">Onde Inference</a> · <a href="https://apps.apple.com/se/developer/splitfire-ab/id1831430993">Splitfire AB</a></sub>
</p>