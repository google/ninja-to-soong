// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Clpeak {
    src_path: PathBuf,
}

impl Project for Clpeak {
    fn get_name(&self) -> &'static str {
        "clpeak"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        self.src_path = ctx.get_android_path(self)?;
        let build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;

        common::gen_ninja(&self.src_path, &build_path, &[&ndk_path], ctx, self)?;

        SoongPackage::new(
            &["//visibility:public"],
            "clpeak_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[target_typed!("clpeak", "cc_benchmark", "clpeak")]),
            parse_build_ninja::<CmakeNinjaTarget>(&build_path)?,
            &self.src_path,
            &ndk_path,
            &build_path,
            None,
            self,
            ctx,
        )?
        .print(ctx)
    }

    fn extend_module(&self, _target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        module
            .extend_prop("test_suites", vec!["dts"])?
            .extend_prop("header_libs", vec!["OpenCL-CLHPP"])?
            .add_prop("soc_specific", true)
            .extend_prop("cflags", vec!["-fexceptions"])
    }

    fn map_lib(&self, lib: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        if lib.ends_with("libOpenCL") {
            return Some((
                PathBuf::from("//external/OpenCL-ICD-Loader:libOpenCL"),
                kind,
            ));
        }
        None
    }

    fn filter_cflag(&self, cflag: &str) -> bool {
        cflag.contains("VERSION_STR")
    }
    fn filter_include(&self, include: &Path) -> bool {
        include.starts_with(&self.src_path)
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_lib(&self, lib: &str) -> bool {
        lib.contains("libOpenCL")
    }
}
