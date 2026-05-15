#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

bash "${SCRIPT_DIR}/../checkout.sh" https://github.com/google/clspv 1d7b14e9ae1423e6a8a5428aea03906f87bf0dac "${DEST}/external/clspv"
