// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Mesa3DDesktopIntel {
    src_path: PathBuf,
    assets_to_filter: Vec<PathBuf>,
}

impl Mesa3DDesktopIntel {
    fn get_intel_tools_targets(&self, build_path: &Path) -> Result<Vec<NinjaTargetToGen>, String> {
        Ok(ls_dir(&build_path.join("src/intel/tools"))?
            .into_iter()
            .filter_map(|entry| {
                let name = file_stem(&entry);
                if name.starts_with("lib") {
                    return None;
                }
                Some(target!(
                    format!("src/intel/tools/{name}"),
                    format!("desktop_mesa3d_intel_tools_{name}"),
                    name
                ))
            })
            .collect::<Vec<_>>())
    }
}

impl mesa3d_desktop::Mesa3dProject for Mesa3DDesktopIntel {
    fn get_name(&self) -> &'static str {
        "desktop/mesa3d/intel"
    }

    fn get_subprojects_path(&self) -> String {
        path_to_string(&self.src_path.join("subprojects"))
    }

    fn asset_filter(&self, asset: &Path) -> bool {
        !self.assets_to_filter.contains(&PathBuf::from(asset))
            && !path_to_string(asset).contains("expat")
    }

    fn create_package(
        &mut self,
        ctx: &Context,
        src_path: &Path,
        build_path: &Path,
        ndk_path: &Path,
        meson_generated: &str,
        targets_map: NinjaTargetsMap<MesonNinjaTarget>,
    ) -> Result<SoongPackage, String> {
        self.src_path = PathBuf::from(src_path);

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
        targets.extend(self.get_intel_tools_targets(build_path)?);
        let targets_to_gen = NinjaTargetsToGenMap::from(&targets);
        self.assets_to_filter = Self::extract_assets_to_filter(&targets_to_gen, &targets_map)?;
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
        .generate_from_map(
            targets_to_gen,
            targets_map,
            &self.src_path,
            &ndk_path,
            &build_path,
            Some(meson_generated),
            self,
            ctx,
        )
    }

    fn get_default_module(&self, package: &SoongPackage) -> Result<SoongModule, String> {
        Ok(SoongModule::new_cc_defaults(CcDefaults::Mesa3DIntel)
            .add_props(package.get_props("desktop_mesa3d_intel_pps-producer", vec!["cflags"])?)
            .add_defaults(CcDefaults::Mesa3DIntelManual)?)
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
        if target.ends_with("libvulkan_intel.so") {
            module = module
                .add_prop("relative_install_path", SoongProp::Str(String::from("hw")))
                .add_prop("afdo", SoongProp::Bool(true));
        }

        if target.ends_with("libintel_decoder.a") {
            module = module
                .extend_prop("static_libs", vec!["libexpat"])?
                .extend_prop("shared_libs", vec!["libz"])?;
        }

        if target.ends_with("libmesa_util.a") {
            module = module.extend_prop("shared_libs", vec!["libz"])?;
        }
        module = if ![
            "libintel_decoder_brw.a",
            "libintel_decoder_elk.a",
            "libintel_decoder_stub_brw.a",
        ]
        .contains(&file_name(target).as_str())
        {
            module.add_defaults(CcDefaults::Mesa3DIntel)?
        } else {
            module.add_defaults(CcDefaults::Mesa3DIntelManual)?
        };
        Ok(module)
    }
}
