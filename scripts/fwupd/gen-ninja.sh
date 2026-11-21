#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 5 ]

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
SRC_PATH="$1"
BUILD_PATH="$2"
NDK_PATH="$3"
TEST_PATH="$4"
COPY_TO_AOSP="$5"

if [ -z "${COPY_TO_AOSP}" ]; then
    mkdir -p "${BUILD_PATH}"
    cp -r "${TEST_PATH}/build.ninja" "${BUILD_PATH}"
    exit 0
fi

MESON_LOCAL_PATH="${HOME}/.local/share/meson/cross"
AOSP_AMD64="aosp-amd64"
ANDROID_PLATFORM="$(bash "${SCRIPT_DIR}/../get_android_platform.sh" "${NDK_PATH}")"
mkdir -p "${MESON_LOCAL_PATH}"
ANDROID_PLATFORM="${ANDROID_PLATFORM}" NDK_PATH="${NDK_PATH}" \
envsubst < "${SCRIPT_DIR}/${AOSP_AMD64}.template" > "${MESON_LOCAL_PATH}/${AOSP_AMD64}"

meson setup \
    --cross-file "${AOSP_AMD64}" \
    --prefix=/data/fwupd \
    --reconfigure \
    --wipe \
    "${BUILD_PATH}" \
    "${SRC_PATH}"

REAL_SRC_PATH="$(grep -e '--internal regenerate' ${BUILD_PATH}/build.ninja | sed 's|^.*--internal regenerate \([^ ]*\) .*|\1|')"
sed "s|../../..${REAL_SRC_PATH}|/ninja-to-soong-fwupd|g" "${BUILD_PATH}/build.ninja" | sed "s|${REAL_SRC_PATH}|/ninja-to-soong-fwupd|g" | sed "s|${HOME}|/home|g" > "${TEST_PATH}/build.ninja"
