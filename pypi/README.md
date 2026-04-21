<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde/main/assets/onde-inference-logo.svg" alt="Onde Inference" width="96">
</p>

<h1 align="center">onde-cli</h1>

<p align="center">
  <strong>Terminal UI for <a href="https://ondeinference.com">Onde Inference</a> — sign up, sign in, and manage your account from the command line.</strong>
</p>

<p align="center">
  <a href="https://ondeinference.com"><img src="https://img.shields.io/badge/ondeinference.com-235843?style=flat-square&labelColor=17211D" alt="Website"></a>
  <a href="https://pypi.org/project/onde-cli/"><img src="https://img.shields.io/pypi/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="PyPI"></a>
  <a href="https://crates.io/crates/onde-cli"><img src="https://img.shields.io/crates/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="Crates.io"></a>
  <a href="https://github.com/ondeinference/onde-cli/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-235843?style=flat-square&labelColor=17211D" alt="License"></a>
</p>

---

## Install

```sh
pip install onde-cli
```

This installs the native `onde` executable for your platform directly — no compiler, no runtime dependencies, no Node.js.

## Usage

```sh
onde
```

Opens a terminal UI. Create an account or sign in to your Onde Inference account.

## Keys

| Key      | Action                |
|----------|-----------------------|
| `Tab`    | Move between fields   |
| `Enter`  | Submit / sign out     |
| `Ctrl+L` | Go to sign in         |
| `Ctrl+N` | Go to new account     |
| `Ctrl+C` | Quit                  |

## Other Installation Methods

### Cargo

```sh
cargo install onde-cli
```

### From source

```sh
git clone https://github.com/ondeinference/onde-cli
cd onde-cli
cargo build --release
./target/release/onde
```

## Platform Support

Pre-built native binaries are shipped for:

| Platform      | Architecture  |
|---------------|---------------|
| macOS         | arm64, x64    |
| Linux (glibc) | arm64, x64    |
| Windows       | arm64, x64    |

## Debug

Logs are written to `~/.cache/onde/debug.log`. Nothing is written to the terminal output.

## Related

- [Swift SDK](https://github.com/ondeinference/onde-swift)
- [Flutter SDK](https://pub.dev/packages/onde_inference)
- [React Native SDK](https://www.npmjs.com/package/@ondeinference/react-native)
- [Onde Rust crate](https://crates.io/crates/onde)

## Source & Issues

This is a native binary distributed via PyPI. The source code lives at
[github.com/ondeinference/onde-cli](https://github.com/ondeinference/onde-cli).
Please report bugs and feature requests there.

## License

Dual-licensed under **MIT** and **Apache 2.0**.

- [MIT License](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-MIT)
- [Apache License 2.0](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-APACHE)

---

<p align="center">
  <sub>© 2026 <a href="https://ondeinference.com">Onde Inference</a></sub>
</p>