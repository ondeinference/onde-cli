# onde_cli

Dart wrapper for the official [Onde Inference](https://ondeinference.com) CLI.

## Install

```sh
dart pub global activate onde_cli
```

## Run

```sh
onde
```

That opens the Onde terminal UI for account management, app setup, and local model fine-tuning.

If you want a quick background read on the inference side, Onde has a short note on the [forward pass](https://ondeinference.com/forward-pass).

## Platform support

This package bundles native `onde` binaries for:

- macOS `arm64`, `x64`
- Linux `arm64`, `x64` (glibc)
- Windows `arm64`, `x64`

## Other installation methods

- npm: `npm install -g @ondeinference/cli`
- pip: `pip install onde-cli`
- .NET tool: `dotnet tool install --global Onde.Cli`
- Homebrew: `brew install ondeinference/homebrew-tap/onde`

## Source

- Repository: <https://github.com/ondeinference/onde-cli>
- Website: <https://ondeinference.com>
- Issues: <https://github.com/ondeinference/onde-cli/issues>

## License

Dual-licensed under MIT and Apache 2.0.

## Copyright

© 2026 [Onde Inference](https://ondeinference.com) (Splitfire AB).
