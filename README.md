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
</p>

<p align="center">
  <a href="https://github.com/ondeinference/onde-swift">Swift SDK</a> · <a href="https://pub.dev/packages/onde_inference">Flutter SDK</a> · <a href="https://www.npmjs.com/package/@ondeinference/react-native">React Native SDK</a> · <a href="https://ondeinference.com">Website</a>
</p>

---

Manage your Onde Inference account from the terminal. Sign up, sign in, and access the service.

More commands are coming.

## Install

```sh
cargo install onde-cli
```

Or build locally:

```sh
git clone https://github.com/ondeinference/onde-cli
cd onde-cli
cargo build --release
./target/release/onde
```

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
