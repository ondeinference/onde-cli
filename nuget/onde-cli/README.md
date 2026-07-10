# Onde CLI for .NET

`onde` is the official command-line interface for [Onde Inference](https://ondeinference.com).

## Install

```sh
dotnet tool install --global Onde.Cli
```

## Update

```sh
dotnet tool update --global Onde.Cli
```

## Run

```sh
onde
```

That opens the terminal UI for account management, app setup, and local model fine-tuning. For package details, platform notes, and the full install matrix, see <https://ondeinference.com/cli>.

If you want a quick background read on the inference side, Onde has a short note on the [forward pass](https://ondeinference.com/forward-pass).

## Platform support

This .NET tool bundles native `onde` binaries for:

- macOS `arm64`, `x64`
- Linux `arm64`, `x64` (glibc)
- Windows `arm64`, `x64`

## Other installation methods

- npm: `npm install -g @ondeinference/cli`
- pip: `pip install onde-cli`
- Dart pub: `dart pub global activate onde_cli`
- Homebrew: `brew install ondeinference/homebrew-tap/onde`
- GitHub Releases: <https://github.com/ondeinference/onde-cli/releases>

## Source

- Repository: <https://github.com/ondeinference/onde-cli>
- Website: <https://ondeinference.com>
- Issues: <https://github.com/ondeinference/onde-cli/issues>

## License

Dual-licensed under MIT and Apache 2.0.

## Copyright

© 2026 [Splitfire AB](https://5mb.app) ([Onde Inference](https://ondeinference.com)).
