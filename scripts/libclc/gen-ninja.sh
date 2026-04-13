#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 3 ]

SRC_PATH="$1"
BUILD_PATH="$2"
TARGET="$3"

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

cmake -G Ninja \
    -S "${SRC_PATH}" \
    -B "${BUILD_PATH}" \
    -DLLVM_DIR="${SCRIPT_DIR}" \
    -DCMAKE_CLC_COMPILER="${SCRIPT_DIR}/clang" \
    -DLIBCLC_TARGET="${TARGET}"
