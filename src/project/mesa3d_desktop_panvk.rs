// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct PanVK;
pub type Mesa3DDesktopPanVK = mesa3d_desktop::Mesa3dDesktop<PanVK>;

impl mesa3d_desktop::Mesa3dProject for PanVK {
    fn get_name(&self) -> &'static str {
        "desktop/mesa3d/panvk"
    }

    fn get_targets(&self, _build_path: &Path) -> Result<Vec<NinjaTargetToGen>, String> {
        Ok(vec![
            target!(
                "src/panfrost/vulkan/libvulkan_panfrost.so",
                "desktop-mesa3d_panvk_libvulkan_panfrost",
                "vulkan.panfrost"
            ),
            target!(
                "src/tool/pps/pps-producer",
                "desktop-mesa3d_panvk_pps-producer",
                "pps-producer"
            ),
            target!(
                "src/tool/pps/libgpudataproducer.so",
                "desktop-mesa3d_panvk_libgpudataproducer",
                "libgpudataproducer_panfrost"
            ),
        ])
    }

    fn create_package(&self) -> SoongPackage {
        SoongPackage::new(
            &["//visibility:public"],
            "mesa3d_desktop_panvk_licenses",
            &[
                "SPDX-license-identifier-Apache-2.0",
                "SPDX-license-identifier-MIT",
                "SPDX-license-identifier-BSL-1.0",
            ],
            &["licenses/Apache-2.0", "licenses/MIT", "licenses/BSL-1.0"],
        )
    }

    fn get_defaults(&self) -> (CcDefaults, CcDefaults) {
        (CcDefaults::Mesa3DPanvk, CcDefaults::Mesa3DPanvkManual)
    }

    fn get_raw_suffix(&self, common_raw_prop: &'static str) -> String {
        format!(
            r#"
cc_defaults {{
    name: "{}",
    soc_specific: true,
    header_libs: ["libdrm_headers"],
    static_libs: ["libperfetto_client_experimental"],
{common_raw_prop}
}}
"#,
            CcDefaults::Mesa3DPanvkManual.str()
        )
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        module.update_prop("generated_headers", |prop| {
            let SoongProp::VecStr(mut vec) = prop else {
                return Ok(prop);
            };
            vec.push(path_to_id(
                Path::new(self.get_name()).join("src/util/shader_stats.h"),
            ));
            Ok(SoongProp::VecStr(vec))
        })?;

        let mut cflags = vec![
            "-Wno-constant-conversion",
            "-Wno-enum-conversion",
            "-Wno-error",
            "-Wno-ignored-qualifiers",
            "-Wno-initializer-overrides",
            "-Wno-macro-redefined",
            "-Wno-non-virtual-dtor",
            "-Wno-pointer-arith",
            "-Wno-unused-parameter",
        ];
        if target.ends_with("libvulkan_lite_runtime.a") {
            cflags.push("-Wno-unreachable-code-loop-increment");
        }
        module
            .add_defaults(CcDefaults::Mesa3DPanvk)?
            .extend_prop("cflags", cflags)
    }
}
