#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 6 ]

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
SRC_PATH="$1"
BUILD_PATH="$2"
NDK_PATH="$3"
TEST_PATH="$4"
COPY_TO_AOSP="$5"
ARCH="$6"

TEST_BUILD_NINJA="${TEST_PATH}/build.${ARCH}.ninja"

if [ -z "${COPY_TO_AOSP}" ]; then
    mkdir -p "${BUILD_PATH}"
    cp -r "${TEST_BUILD_NINJA}" "${BUILD_PATH}/build.ninja"
    exit 0
fi

# Use the same NDK as fwupd's Android CI, e.g. N2S_NDK=android-ndk-r27d.
FWUPD_NDK="android-ndk-$(sed -n 's/^NDK_VERSION_NAME = "\(.*\)"$/\1/p' "${SRC_PATH}/contrib/ci/android.py")"
if [ "$(basename "${NDK_PATH}")" != "${FWUPD_NDK}" ]; then
    echo "fwupd is built with ${FWUPD_NDK} (see contrib/ci/android.py), set N2S_NDK=${FWUPD_NDK}"
    exit 1
fi

MESON_LOCAL_PATH="${HOME}/.local/share/meson/cross"
CROSS_FILE="aosp-${ARCH}"
case "${ARCH}" in
    x86_64) MESON_ARCH="x86_64" ;;
    arm64) MESON_ARCH="aarch64" ;;
    *) echo "Unsupported architecture: ${ARCH}" && exit 1 ;;
esac
ANDROID_PLATFORM="$(bash "${SCRIPT_DIR}/../get_android_platform.sh" "${NDK_PATH}")"
mkdir -p "${MESON_LOCAL_PATH}"
ANDROID_PLATFORM="${ANDROID_PLATFORM}" MESON_ARCH="${MESON_ARCH}" NDK_PATH="${NDK_PATH}" \
envsubst < "${SCRIPT_DIR}/aosp.template" > "${MESON_LOCAL_PATH}/${CROSS_FILE}"

meson setup \
    --cross-file "${CROSS_FILE}" \
    --prefix=/vendor \
    --sysconfdir=/vendor/etc \
    --datadir=/vendor/usr/share \
    --localstatedir=/data/vendor/fwupd \
    --reconfigure \
    --wipe \
    "${BUILD_PATH}" \
    "${SRC_PATH}"

REAL_SRC_PATH="$(grep -e '--internal regenerate' ${BUILD_PATH}/build.ninja | sed 's|^.*--internal regenerate \([^ ]*\) .*|\1|')"
# meson refers to the sources relative to the build directory.
REL_SRC_PATH="$(realpath --relative-to="${BUILD_PATH}" "${REAL_SRC_PATH}")"
sed "s|${REL_SRC_PATH//./\\.}|/ninja-to-soong-fwupd|g" "${BUILD_PATH}/build.ninja" | sed "s|${REAL_SRC_PATH}|/ninja-to-soong-fwupd|g" | sed "s|${HOME}|/home|g" > "${TEST_BUILD_NINJA}"
