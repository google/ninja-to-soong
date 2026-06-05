// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Mesa3DDesktopPanVK {
    src_path: PathBuf,
    assets_to_filter: Vec<PathBuf>,
}

impl mesa3d_desktop::Mesa3dProject for Mesa3DDesktopPanVK {
    fn get_name(&self) -> &'static str {
        "desktop/mesa3d/panvk"
    }

    fn get_subprojects_path(&self) -> String {
        path_to_string(&self.src_path.join("subprojects"))
    }

    fn asset_filter(&self, asset: &Path) -> bool {
        !self.assets_to_filter.contains(&PathBuf::from(asset))
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
        let targets_to_gen = NinjaTargetsToGenMap::from(&[
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
        ]);
        self.assets_to_filter = Self::extract_assets_to_filter(&targets_to_gen, &targets_map)?;
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
        Ok(SoongModule::new_cc_defaults(CcDefaults::Mesa3DPanvk)
            .add_props(package.get_props("desktop-mesa3d_panvk_pps-producer", vec!["cflags"])?)
            .add_defaults(CcDefaults::Mesa3DPanvkManual)?)
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
                Path::new(mesa3d_desktop::Mesa3dProject::get_name(self))
                    .join("src/util/shader_stats.h"),
            ));
            Ok(SoongProp::VecStr(vec))
        })?;

        if target.ends_with("libvulkan_panfrost.so") {
            module = module
                .add_prop("relative_install_path", SoongProp::Str(String::from("hw")))
                .add_prop("afdo", SoongProp::Bool(true))
        }

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
        if target.ends_with("libmesa_util.a") {
            module = module.extend_prop("shared_libs", vec!["libz"])?;
        }
        module
            .add_defaults(CcDefaults::Mesa3DPanvk)?
            .extend_prop("cflags", cflags)
    }
}
