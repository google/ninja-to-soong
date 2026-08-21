// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Vkoverhead();

impl Project for Vkoverhead {
    fn get_name(&self) -> &'static str {
        "vkoverhead"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        let src_path = ctx.get_android_path(self)?;
        let build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;
        let mesa_path = ctx.get_android_path(self)?.parent().unwrap().join("mesa3d");

        common::gen_ninja(
            &src_path,
            &build_path,
            vec![path_to_string(&ndk_path), path_to_string(&mesa_path)],
            ctx,
            self,
        )?;

        const MESON_GENERATED: &str = "meson_generated";
        let mut package = SoongPackage::new(
            &[],
            "vkoverhead_license",
            &["SPDX-license-identifier-MIT"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[target!("vkoverhead", "vkoverhead")]),
            parse_build_ninja::<MesonNinjaTarget>(&build_path)?,
            &src_path,
            &ndk_path,
            &build_path,
            Some(MESON_GENERATED),
            self,
            ctx,
        )?;

        package.filter_gen_deps(MESON_GENERATED, &vec![])?;
        package.print(ctx)
    }

    fn extend_module(&self, _target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        module
            .extend_prop(
                "header_libs",
                vec![
                    "mesa_common_headers",
                    "libcutils_headers",
                    "libhardware_headers",
                    "liblog_headers",
                ],
            )?
            .extend_prop("cflags", vec!["-Wno-error"])
    }
    fn extend_python_binary_host(
        &self,
        _python_binary_path: &Path,
        module: SoongModule,
    ) -> Result<SoongModule, String> {
        Ok(module.add_prop("libs", SoongProp::VecStr(vec![String::from("mako")])))
    }

    fn map_lib(&self, library: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        let str = path_to_string(library);
        if str.contains("libmesa_util.a") {
            Some((PathBuf::from("mesa_util"), LibraryKind::Static))
        } else if str.contains("libmesa_util_c11.a") {
            Some((PathBuf::from("mesa_util_c11"), LibraryKind::Static))
        } else if str.contains("android_stub") || !str.starts_with("src") {
            Some((PathBuf::from(file_stem(library)), kind))
        } else {
            None
        }
    }

    fn map_cmd_output(&self, output: &Path) -> Option<String> {
        Some(file_name(output))
    }

    fn filter_target(&self, target: &Path) -> bool {
        !path_to_string(target).contains("subprojects")
    }
    fn filter_lib(&self, lib: &str) -> bool {
        !lib.contains("clflush") && !lib.contains("blake3") && !lib.contains("simd")
    }
    fn filter_include(&self, include: &Path) -> bool {
        !path_to_string(include).contains("subprojects")
    }
    fn filter_gen_header(&self, header: &Path) -> bool {
        !path_to_string(header).contains("subprojects")
    }
    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
}
