<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde/main/assets/onde-inference-logo.svg" alt="Onde Inference" width="80">
</p>

<h1 align="center">onde</h1>

<p align="center">
  <strong>Manage your <a href="https://ondeinference.com">Onde Inference</a> apps from the terminal.</strong><br>
  Sign up, sign in, assign models — no browser required.
</p>

<p align="center">
  <a href="https://ondeinference.com"><img src="https://img.shields.io/badge/ondeinference.com-235843?style=flat-square&labelColor=17211D" alt="Website"></a>
  <a href="https://pypi.org/project/onde-cli/"><img src="https://img.shields.io/pypi/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="PyPI"></a>
  <a href="https://crates.io/crates/onde-cli"><img src="https://img.shields.io/crates/v/onde-cli?style=flat-square&labelColor=17211D&color=235843" alt="Crates.io"></a>
  <a href="https://github.com/ondeinference/onde-cli/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-235843?style=flat-square&labelColor=17211D" alt="License"></a>
</p>

<br>

<p align="center">
  <img src="https://raw.githubusercontent.com/ondeinference/onde-cli/main/assets/screenshot.png" alt="onde CLI — apps list with model assignments" width="720">
</p>

<br>

---

## Install

```sh
pip install onde-cli
```

Installs the native `onde` binary for your platform — no compiler, no Node.js, no runtime dependencies.

## Quick start

```sh
onde
```

A full terminal UI opens. Create an account or sign in, then manage your apps and model assignments without leaving the terminal.

## Keys

### Auth screen

| Key       | Action                    |
|-----------|---------------------------|
| `Tab`     | Move between fields       |
| `Enter`   | Submit form               |
| `Ctrl+L`  | Switch to Sign in         |
| `Ctrl+N`  | Switch to Create account  |
| `Ctrl+C`  | Quit                      |

### Apps list

| Key       | Action                    |
|-----------|---------------------------|
| `↑` `↓`  | Navigate apps             |
| `Enter`   | Open app detail           |
| `n`       | Create new app            |
| `s`       | Sign out                  |
| `Ctrl+C`  | Quit                      |

### App detail

| Key       | Action                    |
|-----------|---------------------------|
| `m`       | Assign / change model     |
| `r`       | Rename app                |
| `s`       | Sign out                  |
| `Esc`     | Back to apps list         |

### Model picker

| Key       | Action                    |
|-----------|---------------------------|
| `↑` `↓`  | Navigate models           |
| `Enter`   | Assign selected model     |
| `Esc`     | Cancel                    |

## Other installation methods

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

## Platform support

Pre-built native binaries ship for every major platform:

| Platform      | Architecture |
|---------------|--------------|
| macOS         | arm64, x64   |
| Linux (glibc) | arm64, x64   |
| Windows       | arm64, x64   |

## Debug logs

Logs are written to `~/.cache/onde/debug.log`. Nothing touches the terminal output — ratatui owns the screen exclusively while the TUI is open.

## Related

| SDK | Install |
|-----|---------|
| [Swift SDK](https://github.com/ondeinference/onde-swift) | Swift Package Manager |
| [Flutter SDK](https://pub.dev/packages/onde_inference) | `flutter pub add onde_inference` |
| [React Native SDK](https://www.npmjs.com/package/@ondeinference/react-native) | `npm i @ondeinference/react-native` |
| [Rust crate](https://crates.io/crates/onde) | `cargo add onde` |

## Source & issues

This package ships a pre-built native binary. Source lives at
[github.com/ondeinference/onde-cli](https://github.com/ondeinference/onde-cli) —
file bugs and feature requests there.

## License

Dual-licensed under **MIT** and **Apache 2.0**.

- [MIT License](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-MIT)
- [Apache License 2.0](https://github.com/ondeinference/onde-cli/blob/main/LICENSE-APACHE)

---

<p align="center">
  <sub>Built by <a href="https://ondeinference.com">Onde Inference</a> · © 2026</sub>
</p>