// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Intel;
pub type Mesa3DDesktopIntel = mesa3d_desktop::Mesa3dDesktop<Intel>;

impl mesa3d_desktop::Mesa3dProject for Intel {
    fn get_name(&self) -> &'static str {
        "desktop/mesa3d/intel"
    }

    fn asset_filter(&self, asset: &Path) -> bool {
        !path_to_string(asset).contains("expat")
    }

    fn get_targets(&self, build_path: &Path) -> Result<Vec<NinjaTargetToGen>, String> {
        let mut targets = vec![
            target!(
                "src/intel/vulkan/libvulkan_intel.so",
                "desktop_mesa3d_intel_libvulkan_intel",
                "vulkan.intel"
            ),
            target!(
                "src/tool/pps/pps-producer",
                "desktop_mesa3d_intel_pps-producer",
                "pps-producer"
            ),
            target!(
                "src/tool/pps/libgpudataproducer.so",
                "desktop_mesa3d_intel_libgpudataproducer",
                "libgpudataproducer"
            ),
        ];
        targets.extend(
            ls_dir(&build_path.join("src/intel/tools"))?
                .into_iter()
                .filter_map(|entry| {
                    let name = file_stem(&entry);
                    (!name.starts_with("lib")).then(|| {
                        target!(
                            format!("src/intel/tools/{name}"),
                            format!("desktop_mesa3d_intel_tools_{name}"),
                            name
                        )
                    })
                }),
        );
        Ok(targets)
    }

    fn create_package(&self) -> SoongPackage {
        SoongPackage::new(
            &["//visibility:public"],
            "desktop_mesa3d_intel_licenses",
            &[
                "SPDX-license-identifier-MIT",
                "SPDX-license-identifier-Apache-2.0",
                "SPDX-license-identifier-GPL-1.0-or-later",
                "SPDX-license-identifier-GPL-2.0-only",
            ],
            &[
                "licenses/MIT",
                "licenses/Apache-2.0",
                "licenses/GPL-1.0-or-later",
                "licenses/GPL-2.0-only",
            ],
        )
    }

    fn get_defaults(&self) -> (CcDefaults, CcDefaults) {
        (CcDefaults::Mesa3DIntel, CcDefaults::Mesa3DIntelManual)
    }

    fn get_raw_suffix(&self, common_raw_prop: &'static str) -> String {
        format!(
            r#"
cc_defaults {{
    name: "{}",
    cflags: ["-Wno-error"],
    soc_specific: true,
    static_libs: [
        "libperfetto_client_experimental",
    ],
    header_libs: [
        "libcutils_headers",
        "libhardware_headers",
        "liblog_headers",
        "libdrm_headers",
    ],
{common_raw_prop}
}}
"#,
            CcDefaults::Mesa3DIntelManual.str()
        )
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        if target.ends_with("libintel_decoder.a") {
            module = module.extend_prop("static_libs", vec!["libexpat"])?;
        }

        if ![
            "libintel_decoder_brw.a",
            "libintel_decoder_elk.a",
            "libintel_decoder_stub_brw.a",
        ]
        .contains(&file_name(target).as_str())
        {
            module.add_defaults(CcDefaults::Mesa3DIntel)
        } else {
            module.add_defaults(CcDefaults::Mesa3DIntelManual)
        }
    }
}
