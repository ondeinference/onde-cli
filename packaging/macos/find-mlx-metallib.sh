#!/usr/bin/env bash
# Print the path of the mlx.metallib that mlx-sys built for a cargo target.
#
# MLX loads its Metal kernels from mlx.metallib next to the onde binary. The
# build only leaves the file under target/, so every macOS arm64 package has
# to copy it from there beside the binary it ships. Run it from the repo
# root; it prints an absolute path.
#
#   packaging/macos/find-mlx-metallib.sh aarch64-apple-darwin [release|debug]
#
# The build profile defaults to release. A set CARGO_TARGET_DIR is honoured.
set -euo pipefail

target="${1:?usage: find-mlx-metallib.sh <cargo target> [profile]}"
profile="${2:-release}"
root="${CARGO_TARGET_DIR:-target}"

# Cargo's build-dir layout changed: older toolchains write
# build/mlx-sys-<hash>/out/..., newer ones write build/mlx-sys/<hash>/out/...
# Look in both. The cargo cache can hold more than one mlx-sys build; take the
# newest.
# shellcheck disable=SC2012
lib="$(ls -t "${root}"/"${target}"/"${profile}"/build/mlx-sys-*/out/build/lib/mlx.metallib \
             "${root}"/"${target}"/"${profile}"/build/mlx-sys/*/out/build/lib/mlx.metallib 2>/dev/null | head -n 1 || true)"
if [[ -z "${lib}" || ! -f "${lib}" ]]; then
  echo "mlx.metallib not found under ${root}/${target}/${profile}/build" >&2
  exit 1
fi
case "${lib}" in
  /*) echo "${lib}" ;;
  *) echo "${PWD}/${lib}" ;;
esac
