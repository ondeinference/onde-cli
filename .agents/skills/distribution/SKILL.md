---
name: distribution
description: Use when adding, fixing, or documenting package distribution for Onde CLI. Covers wrapper-package design across npm, PyPI, NuGet, pub.dev, GitHub Releases, and Homebrew, plus version alignment, artifact staging, README surfaces, and smoke-test strategy.
---

# Onde CLI distribution

This skill captures how `onde-cli` is distributed today and what to watch when adding new channels.

## Distribution model

`onde-cli` is a native Rust CLI first.

Everything else is packaging.

That means:

- build the Rust binary once per target
- publish thin wrappers that install or launch that binary
- keep channel-specific logic small and predictable

This design keeps behavior consistent across ecosystems and avoids re-implementing CLI logic in each language.

## Channel map

### Native source of truth

- Rust crate: `onde-cli`
- binary name: `onde`
- root manifest: `Cargo.toml`

### Wrapper channels

- npm: JavaScript launcher + optional platform packages
- PyPI: `maturin` binary wheels
- NuGet: .NET global tool launcher
- pub.dev: Dart launcher package

### Native artifact channels

- crates.io
- GitHub Releases
- Homebrew

## Wrapper design patterns

### npm

Use a very small Node launcher that:

- detects OS and architecture
- resolves the installed platform package
- spawns the binary with inherited stdio

Good fit for users who already live in npm tooling.

Important release detail:

- the committed `npm/onde-cli/package.json` is a working file, not the publish-time source of truth
- the release workflow rewrites the base package manifest with `npm/scripts/render-main-package.cjs`
- platform package manifests are generated on the fly with `npm/scripts/render-platform-package.cjs`
- the actual npm version comes from the release tag via `RELEASE_VERSION`, not from manually editing the committed npm manifest before publish

### PyPI

Use `maturin` with `bindings = "bin"` so the native executable is what gets installed.

Good fit for Python users who want the CLI but not a Python reimplementation.

### NuGet

Use a `.NET global tool` wrapper.

Important details:

- keep the package as a launcher, not a managed rewrite
- place native binaries under the tool payload
- resolve the runtime identifier at startup
- set Unix execute bits before launch

Good fit for .NET developers and CI environments that already use `dotnet tool install`.

### pub.dev

Use a Dart package with a command entrypoint and a library that launches the native binary.

Important details:

- executable command should stay `onde`
- keep the runtime-id mapping explicit
- download the correct binary from GitHub Releases on first run
- cache downloads under `~/.onde/cli`
- keep the wrapper thin, the Rust binary is still the product

Good fit for Dart and Flutter developers who want the same CLI through familiar tooling.

## Runtime identifier mapping

Keep these names aligned anywhere wrappers or workflows refer to native assets:

- `darwin-x64`
- `darwin-arm64`
- `linux-x64`
- `linux-arm64`
- `windows-x64`
- `windows-arm64`

If a wrapper uses one naming scheme and a workflow uploads another, the installed package will pass packaging but fail at runtime.

## Release workflow principles

### Build first, package second

Always build native binaries in a target matrix first.

Then let the wrapper channel jobs package those artifacts.

This avoids rebuilding the product multiple times and makes failures easier to reason about.

### Idempotency

Every publish workflow should check whether the target version already exists before publishing.

Examples:

- npm: `npm view`
- PyPI: package JSON endpoint
- NuGet: flat container index
- pub.dev: package API endpoint

### Smoke tests should match the product shape

Because `onde` is TUI-first, avoid smoke tests that assume a traditional CLI help flow unless you know that code path is stable and non-interactive.

Safer checks:

- install succeeds
- bundled native binary exists where expected
- package manager validation passes

## pub.dev-specific notes

pub.dev behaves differently from npm and NuGet in one important way:

- large bundled binaries quickly hit package size limits

That makes the safest design a thin Dart launcher that downloads the right native binary from GitHub Releases at runtime instead of shipping every platform binary inside the package.

Practical consequences:

- `pub/onde_cli/native/` should stay out of the published package
- do not set `publish_to: https://pub.dev` in `pubspec.yaml`; omit `publish_to` for normal pub.dev publishing
- use package-level ignore rules so local native staging does not leak into publish input
- keep release asset names aligned with the GitHub Release workflow, because the launcher depends on them
- treat `~/.onde/cli` as the local cache root for downloaded binaries

Run these during verification:

- `dart pub get --directory pub/onde_cli`
- `dart analyze pub/onde_cli`
- `dart pub publish --dry-run --directory pub/onde_cli`

## NuGet-specific notes

NuGet packaging is sensitive to path layout.

The safest pattern is:

- copy native files into the publish output
- let the .NET tool pack process carry them into the final package

Avoid custom `PackagePath` patterns that can duplicate path segments or create nested native folders by accident.

If the tool installs but cannot find the native binary, inspect the `.nupkg` contents before debugging the launcher.

## README surfaces

Every user-facing channel should document the same product with channel-specific installation instructions.

Main files:

- `README.md`
- `npm/onde-cli/README.md`
- `pypi/README.md`
- `nuget/onde-cli/README.md`
- `pub/onde_cli/README.md`

Keep these aligned on:

- product description
- supported platforms
- install method
- license and copyright
- relevant supporting links like the forward-pass explainer

Do not let one channel sound like a different product.

## Version alignment

When cutting a release, update every user-facing distribution version that should move with the CLI.

At minimum, check:

- Rust crate version
- npm release templates and workflow assumptions
- npm lockfile version if committed
- PyPI release version source
- NuGet package version at pack time
- pub.dev package version in `pubspec.yaml`
- workflow example tags and docs that mention concrete versions

## Common failure modes

- wrapper version does not match the Rust release version
- manually bumping the committed npm manifest and assuming that is what npm publishes
- staged native binaries land in the wrong directory before packaging
- package builds successfully but installs without the native binary
- pub.dev launcher points at the wrong GitHub Release asset name or runtime id
- pub.dev package still includes bundled binaries and blows the upload size limit
- smoke test invokes a TUI-first binary in a way that does not make sense for CI
- README badges and install commands drift out of sync with real package names

## Rule of thumb

If the native binary works and the wrappers are boring, distribution stays manageable.

If a wrapper starts acting like its own product, complexity grows fast.

Keep it thin.
Keep it aligned.
Keep the Rust binary as the source of truth.
