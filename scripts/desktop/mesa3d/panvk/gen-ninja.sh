#!/usr/bin/env bash

set -xe

[ $# -eq 4 ]

SRC_PATH="$1"
BUILD_PATH="$2"
MESA_CLC_PATH="$3"
NDK_PATH="$4"

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
MESON_LOCAL_PATH="${HOME}/.local/share/meson/cross"
AOSP_AARCH64="aosp-aarch64"
ANDROID_PLATFORM="$(bash "${SCRIPT_DIR}/../../../get_android_platform.sh" "${NDK_PATH}")"
mkdir -p "${MESON_LOCAL_PATH}"
ANDROID_PLATFORM="${ANDROID_PLATFORM}" NDK_PATH="${NDK_PATH}" \
envsubst < "${SCRIPT_DIR}/${AOSP_AARCH64}.template" > "${MESON_LOCAL_PATH}/${AOSP_AARCH64}"

PKG_CONFIG_PATH="${PKG_CONFIG_PATH}:${SCRIPT_DIR}" \
PATH="${MESA_CLC_PATH}:${PATH}" \
meson setup \
    --cross-file "${AOSP_AARCH64}" \
    --libdir lib64 \
    --sysconfdir=/system/vendor/etc \
    -Dandroid-libbacktrace=disabled \
    -Dllvm=disabled \
    -Degl=disabled \
    -Dplatform-sdk-version=${ANDROID_PLATFORM} \
    -Dandroid-stub=true \
    -Dandroid-libperfetto=enabled \
    -Dplatforms=android \
    -Dperfetto=true \
    -Dcpp_rtti=false \
    -Dtools= \
    -Dvulkan-drivers=panfrost \
    -Dgallium-drivers= \
    -Dvideo-codecs= \
    -Dgles1=disabled \
    -Dgles2=disabled \
    -Dopengl=false \
    -Dbuildtype=release \
    -Dmesa-clc=system \
    -Dprecomp-compiler=system \
    -Dallow-fallback-for=libdrm,perfetto \
    -Dstrip=true \
    -Dspirv-tools=disabled \
    --reconfigure \
    --wipe \
    "${BUILD_PATH}" \
    "${SRC_PATH}"
