#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 3 ]

SRC_PATH="$1"
BUILD_PATH="$2"
NDK_PATH="$3"

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
bash "${SCRIPT_DIR}/../cmake_configure.sh" \
    "${SRC_PATH}" \
    "${BUILD_PATH}" \
    "${NDK_PATH}" \
    -DCLANG_ENABLE_CIR=OFF \
    -DLLVM_ENABLE_PROJECTS="clang" \
    -DLLVM_TARGETS_TO_BUILD=
