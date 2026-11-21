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
    -DCLPEAK_OPENCL_PREDEFINED=ON \
    -DOpenCL_LIBRARIES=OpenCL \
    -DOpenCL_INCLUDE_DIRS="" \
    -DCLPEAK_ENABLE_OPENCL=ON \
    -DCLPEAK_ENABLE_VULKAN=ON \
    -DCLPEAK_ENABLE_LITERT=ON \
    -DCLPEAK_ENABLE_CPU=OFF \
    -DCLPEAK_ENABLE_ONNX=OFF \
    -DCLPEAK_ENABLE_CUDA=OFF \
    -DCLPEAK_ENABLE_ROCM=OFF \
    -DCLPEAK_ENABLE_METAL=OFF \
    -DCLPEAK_ENABLE_ONEAPI=OFF \
    -DCLPEAK_ENABLE_COREML=OFF \
    -DCLPEAK_ENABLE_GUI=OFF \
    -DCLPEAK_GLSLC="${NDK_PATH}/shader-tools/linux-x86_64/glslc"
