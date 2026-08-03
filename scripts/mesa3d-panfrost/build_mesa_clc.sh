#!/usr/bin/env bash

set -xe

[ $# -eq 2 ]

SRC_PATH="$1"
BUILD_PATH="$2"

meson setup \
    -Dplatforms= \
    -Dvulkan-drivers= \
    -Dgallium-drivers= \
    -Dbuildtype=release \
    -Dtools=panfrost \
    -Dmesa-clc=enabled \
    -Dinstall-mesa-clc=true \
    -Dprecomp-compiler=enabled \
    -Dinstall-precomp-compiler=true \
    -Dstrip=true \
    --reconfigure \
    --wipe \
    "${BUILD_PATH}" \
    "${SRC_PATH}"
meson compile -C "${BUILD_PATH}"

mkdir -p "${BUILD_PATH}/bin"
cp "${BUILD_PATH}/src/compiler/clc/mesa_clc"         "${BUILD_PATH}/bin"
cp "${BUILD_PATH}/src/compiler/spirv/vtn_bindgen2"   "${BUILD_PATH}/bin"
cp "${BUILD_PATH}/src/panfrost/clc/panfrost_compile" "${BUILD_PATH}/bin"
