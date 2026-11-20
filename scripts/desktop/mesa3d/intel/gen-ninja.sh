#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 4 ]

SRC_PATH="$1"
BUILD_PATH="$2"
MESA_CLC_PATH="$3"
NDK_PATH="$4"

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
MESON_LOCAL_PATH="${HOME}/.local/share/meson/cross"
AOSP_X86_64="aosp-x86_64"
ANDROID_PLATFORM="37"
mkdir -p "${MESON_LOCAL_PATH}"
ANDROID_PLATFORM="${ANDROID_PLATFORM}" NDK_PATH="${NDK_PATH}" \
envsubst < "${SCRIPT_DIR}/${AOSP_X86_64}.template" > "${MESON_LOCAL_PATH}/${AOSP_X86_64}"

PKG_CONFIG_PATH="${PKG_CONFIG_PATH}:${SCRIPT_DIR}" \
PATH="${MESA_CLC_PATH}:${PATH}" \
meson setup \
    --cross-file "${AOSP_X86_64}" \
    --libdir lib64 \
    --sysconfdir=/system/vendor/etc \
    -Dandroid-libbacktrace=disabled \
    -Dallow-fallback-for=libdrm \
    -Dllvm=disabled \
    -Dglx=disabled \
    -Dgbm=disabled \
    -Degl=enabled \
    -Dplatform-sdk-version=${ANDROID_PLATFORM} \
    -Dandroid-stub=true \
    -Dandroid-libperfetto=enabled \
    -Dplatforms=android \
    -Dperfetto=true \
    -Degl-lib-suffix=_mesa \
    -Dgles-lib-suffix=_mesa \
    -Dcpp_rtti=false \
    -Dtools=intel \
    -Dvulkan-drivers=intel \
    -Dgallium-drivers=iris \
    -Dgallium-rusticl=false \
    -Dgallium-va=disabled \
    -Dbuildtype=release \
    -Dmesa-clc=system \
    -Dintel-rt=enabled \
    -Dintel-elk=false \
    -Dstrip=true \
    -Dspirv-tools=disabled \
    --reconfigure \
    --wipe \
    "${BUILD_PATH}" \
    "${SRC_PATH}"
