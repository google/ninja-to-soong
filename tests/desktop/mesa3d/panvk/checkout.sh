#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
DEST_DIR="${DEST}/vendor/google/graphics/desktop/mesa3d/panvk"

bash "${SCRIPT_DIR}/../../../checkout.sh" https://gitlab.freedesktop.org/mesa/mesa 6226590a3b316badfabc574cde16895e4a4ae62f "${DEST_DIR}"

MAJOR_VERSION=19
sudo apt install \
     bindgen \
     meson-1.5 \
     libclang-${MAJOR_VERSION}-dev \
     libclang-cpp${MAJOR_VERSION}{,-dev} \
     libclc-${MAJOR_VERSION}{,-dev} \
     libllvmspirvlib-${MAJOR_VERSION}-dev \
     llvm-${MAJOR_VERSION}-{dev,tools} \
     glslang-tools \
     python3-{mako,ply}

rustup target add aarch64-linux-android
