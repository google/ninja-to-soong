#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

bash "${SCRIPT_DIR}/../checkout.sh" https://gitlab.freedesktop.org/mesa/mesa 742af792775f21e29e154a4e35b8c1a2e3103de4 "${DEST}/external/mesa3d"
bash "${SCRIPT_DIR}/../checkout.sh" https://github.com/zmike/vkoverhead.git 9f3647dbcdb7a67814933e99834d2864c431c217 "${DEST}/external/vkoverhead"
