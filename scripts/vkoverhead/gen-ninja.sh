#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 4 ]

SRC_PATH="$1"
BUILD_PATH="$2"
NDK_PATH="$3"
MESA_PATH="$4"

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
MESON_LOCAL_PATH="${HOME}/.local/share/meson/cross"
AOSP_X86_64="aosp-x86_64"
ANDROID_PLATFORM="35"
mkdir -p "${MESON_LOCAL_PATH}"
ANDROID_PLATFORM="${ANDROID_PLATFORM}" NDK_PATH="${NDK_PATH}" \
envsubst < "${SCRIPT_DIR}/${AOSP_X86_64}.template" > "${MESON_LOCAL_PATH}/${AOSP_X86_64}"

mkdir -p "${SRC_PATH}/subprojects"
ln -sfn "${MESA_PATH}" "${SRC_PATH}/subprojects/mesa"

meson setup \
    --cross-file "${AOSP_X86_64}" \
    --reconfigure \
    --wipe \
    --wrap-mode=nodownload \
    -Dplatforms=android \
    -Dplatform-sdk-version=${ANDROID_PLATFORM} \
    "${BUILD_PATH}" \
    "${SRC_PATH}"
