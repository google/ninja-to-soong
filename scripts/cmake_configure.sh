#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -ge 3 ]

SRC_PATH="$1"
BUILD_PATH="$2"
NDK_PATH="$3"
shift 3
CMAKE_ARGS=$@

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
ANDROID_PLATFORM="$(bash "${SCRIPT_DIR}/get_android_platform.sh" "${NDK_PATH}")"

cmake -B "${BUILD_PATH}" -S "${SRC_PATH}" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_TOOLCHAIN_FILE="${NDK_PATH}/build/cmake/android.toolchain.cmake" \
    -DANDROID_ABI="arm64-v8a" \
    -DANDROID_PLATFORM="${ANDROID_PLATFORM}" \
    ${CMAKE_ARGS}
