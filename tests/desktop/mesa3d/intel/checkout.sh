#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
DEST_DIR="${DEST}/vendor/google/graphics/desktop/mesa3d/intel"

bash "${SCRIPT_DIR}/../../../checkout.sh" https://gitlab.freedesktop.org/mesa/mesa 3848a080534a17ca075e9e95dd3a14abb80139aa "${DEST_DIR}"

for patch in \
    "mesa-c0f332f1cba.patch" \
    "mesa-a25e88cd84b.patch" \
    "mesa-be9e0f2f6a3.patch"
do
    git -C "${DEST_DIR}" apply "${SCRIPT_DIR}/../patches/${patch}"
done

MAJOR_VERSION=19
sudo apt install \
     meson \
     libclang-${MAJOR_VERSION}-dev \
     libclang-cpp${MAJOR_VERSION}{,-dev} \
     libclc-${MAJOR_VERSION}{,-dev} \
     libllvmspirvlib-${MAJOR_VERSION}-dev \
     llvm-${MAJOR_VERSION}-{dev,tools} \
     glslang-tools \
     python3-{mako,ply}
