<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde/main/assets/onde-inference-logo.svg" alt="Onde Inference" width="96">
</p>

<h1 align="center">onde</h1>

<p align="center">
  <strong>Command-line interface for <a href="https://ondeinference.com">Onde Inference</a>.</strong>
</p>

<p align="center">
  <a href="https://ondeinference.com"><img src="https://img.shields.io/badge/ondeinference.com-235843?style=flat-square&labelColor=17211D" alt="Website"></a>
  <a href="https://apps.apple.com/se/developer/splitfire-ab/id1831430993"><img src="https://img.shields.io/badge/App%20Store-live-235843?style=flat-square&labelColor=17211D" alt="App Store"></a>
  <a href="https://pypi.org/project/onde-cli/"><img src="https://img.shields.io/pypi/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="PyPI"></a>
  <a href="https://crates.io/crates/onde-cli"><img src="https://img.shields.io/crates/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="Crates.io"></a>
</p>

<p align="center">
  <a href="https://github.com/ondeinference/onde-swift">Swift SDK</a> · <a href="https://pub.dev/packages/onde_inference">Flutter SDK</a> · <a href="https://www.npmjs.com/package/@ondeinference/react-native">React Native SDK</a> · <a href="https://ondeinference.com">Website</a>
</p>

---

Manage your Onde Inference account from the terminal. Sign up, sign in, and access the service.

## Install

### Homebrew (macOS)

```sh
brew tap ondeinference/homebrew-tap
brew install onde
```

### pip

```sh
pip install onde-cli
```

### uv

```sh
uv tool install onde-cli
```

### Pre-built binary (macOS, Linux, Windows)

Download the latest binary for your platform from the
[GitHub Releases](https://github.com/ondeinference/onde-cli/releases) page,
make it executable, and move it onto your `PATH`:

```sh
# macOS arm64 example
curl -Lo onde https://github.com/ondeinference/onde-cli/releases/latest/download/onde-macos-arm64
chmod +x onde
mv onde /usr/local/bin/onde
```

| Platform | File |
|---|---|
| macOS Apple silicon | `onde-macos-arm64` |
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

Opens a terminal UI. Create an account or sign in.

## Keys

| Key    | What it does        |
|--------|---------------------|
| Tab    | move between fields |
| Enter  | submit / sign out   |
| Ctrl+L | go to sign in       |
| Ctrl+N | go to new account   |
| Ctrl+C | quit                |

## Debug

Logs go to `~/.cache/onde/debug.log`. Nothing touches the terminal output.

---

## License

Dual-licensed under **MIT** and **Apache 2.0**.

- [MIT License](https://github.com/ondeinference/onde/blob/main/LICENSE-MIT)
- [Apache License 2.0](https://github.com/ondeinference/onde/blob/main/LICENSE-APACHE)

---

<p align="center">
  <sub>© 2026 <a href="https://ondeinference.com">Onde Inference</a></sub>
</p>
