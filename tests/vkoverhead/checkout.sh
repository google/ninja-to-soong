#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

bash "${SCRIPT_DIR}/../checkout.sh" https://gitlab.freedesktop.org/mesa/mesa 742af792775f21e29e154a4e35b8c1a2e3103de4 "${DEST}/external/mesa3d"
bash "${SCRIPT_DIR}/../checkout.sh" https://github.com/zmike/vkoverhead.git a1c3001c519425b05ea580b130ef096427ba26b6 "${DEST}/external/vkoverhead"
