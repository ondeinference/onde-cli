#!/usr/bin/env bash
# Print the path of the mlx.metallib that mlx-sys built for a cargo target.
#
# MLX loads its Metal kernels from mlx.metallib next to the onde binary. The
# build only leaves the file under target/, so every macOS arm64 package has
# to copy it from there beside the binary it ships. Run it from the repo
# root; it prints an absolute path.
#
#   packaging/macos/find-mlx-metallib.sh aarch64-apple-darwin
set -euo pipefail

target="${1:?usage: find-mlx-metallib.sh <cargo target>}"

# The cargo cache can hold more than one mlx-sys build; take the newest.
# shellcheck disable=SC2012
lib="$(ls -t target/"${target}"/release/build/mlx-sys-*/out/build/lib/mlx.metallib 2>/dev/null | head -n 1 || true)"
if [[ -z "${lib}" || ! -f "${lib}" ]]; then
  echo "mlx.metallib not found under target/${target}/release/build" >&2
  exit 1
fi
echo "${PWD}/${lib}"
