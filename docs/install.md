# Install

Pick one. Each method installs an `onde` command on your PATH.

## npm

```sh
npm install -g @ondeinference/cli
```

## Homebrew

```sh
brew tap ondeinference/homebrew-tap && brew trust --tap ondeinference/homebrew-tap
brew install onde
```

## pip / uv / uvx

```sh
pip install onde-cli
# or
uv tool install onde-cli
uv run onde --tui
# or with
uvx --from onde-cli onde --tui
```

## .NET tool

```sh
dotnet tool install --global Onde.Cli
```

## Dart pub global

```sh
dart pub global activate onde_cli
```

The Dart package is a thin launcher. On first run it downloads the right native binary into `~/.onde/cli`, then reuses the local copy.

## winget

```powershell
winget install OndeInference.CLI
```

## Scoop

```powershell
scoop bucket add onde https://github.com/ondeinference/scoop-bucket
scoop install onde
```

## Build from source

```sh
cargo install onde-cli       # from crates.io
# or, in a checkout:
cargo build --release
```

A build you make yourself has no Onde credentials. It fine-tunes, merges, exports to GGUF and evaluates models on your machine, and chat works. Cloud sign-in, publishing to Hugging Face through your Onde account and assigning models to apps are off, and say so when you try them. The binaries on GitHub Releases and in the package managers include the credentials. To get them in a build of your own you need the Onde values in a `.env` file or the environment.

## Pre-built binary

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
