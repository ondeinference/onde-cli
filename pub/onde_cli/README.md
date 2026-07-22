# onde_cli

Dart launcher for the official [Onde Inference](https://ondeinference.com) CLI.

## Install

```sh
dart pub global activate onde_cli
```

## Run

```sh
onde
```

That opens the Onde terminal UI for account management, app setup, and local model fine-tuning. On first run, the launcher downloads the right native binary into `~/.onde/cli`. After that it reuses the local copy. For package details, platform notes, and the full install matrix, see <https://ondeinference.com/cli>.

If you want a quick background read on the inference side, Onde has a short note on the [forward pass](https://ondeinference.com/forward-pass).

## Platform support

This package downloads native `onde` binaries for:

- macOS `arm64`, `x64`
- Linux `arm64`, `x64` (glibc)
- Windows `arm64`, `x64`

## Other installation methods

- npm: `npm install -g @ondeinference/cli`
- pip: `pip install onde-cli`
- .NET tool: `dotnet tool install --global Onde.Cli`
- Homebrew: `brew install ondeinference/homebrew-tap/onde`

## Cache location

The launcher stores downloaded binaries under:

```sh
~/.onde/cli
```

Versioned downloads live under that directory, so different CLI versions can coexist without stepping on each other.

## Source

- Repository: <https://github.com/ondeinference/onde-cli>
- Website: <https://ondeinference.com>
- Issues: <https://github.com/ondeinference/onde-cli/issues>

## License

Dual-licensed under MIT and Apache 2.0.

## Copyright

© 2026 [Splitfire AB](https://5mb.app) ([Onde Inference](https://ondeinference.com)).
