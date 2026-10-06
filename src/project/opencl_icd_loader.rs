// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct OpenclIcdLoader();

const GENERATED_CMAKE_CONFIG: &str = "generate_cmake_config";

impl Project for OpenclIcdLoader {
    fn get_name(&self) -> &'static str {
        "OpenCL-ICD-Loader"
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

        common::gen_ninja(&src_path, &build_path, &[&ndk_path], ctx, self)?;

        SoongPackage::new(
            &["//visibility:public"],
            "OpenCL-ICD-Loader_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[target!("libOpenCL.so", "libOpenCL")]),
            parse_build_ninja::<CmakeNinjaTarget>(&build_path)?,
            &src_path,
            &ndk_path,
            &build_path,
            None,
            self,
            ctx,
        )?
        .add_raw_prefix(
            r#"
soong_namespace {}
"#,
        )
        .add_raw_suffix(&format!(
            r#"
genrule {{
    name: "{GENERATED_CMAKE_CONFIG}",
    out: ["icd_cmake_config.h"],
    /*
        Android's libc doesn't implement `secure_getenv` or `__secure_getenv`
        so we just create an empty $(out) file.
    */
    cmd: "touch $(out)",
    visibility: ["//visibility:private"],
}}
"#
        ))
        .print(ctx)
    }

    fn extend_module(&self, _target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        module
            .add_prop("header_libs", vec!["OpenCL-Headers"])
            .add_prop("export_header_lib_headers", vec!["OpenCL-Headers"])
            .add_prop("generated_headers", vec![GENERATED_CMAKE_CONFIG])
            .add_prop("soc_specific", true)
            .extend_prop(
                "cflags",
                vec![
                    "-DICD_VENDOR_PATH=\\\"/vendor/etc/Khronos/OpenCL/vendors\\\"",
                    "-DLAYER_PATH=\\\"/vendor/etc/Khronos/OpenCL/layers\\\"",
                ],
            )
    }

    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_define(&self, define: &str) -> bool {
        define.starts_with("CL_") || define.starts_with("OPENCL")
    }
    fn filter_include(&self, include: &Path) -> bool {
        path_to_string(include).contains("external/OpenCL-ICD-Loader")
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_lib(&self, _lib: &str) -> bool {
        false
    }
}
