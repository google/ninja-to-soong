// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Clpeak {
    src_path: PathBuf,
    build_path: PathBuf,
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
        self.build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;

        common::gen_ninja(&self.src_path, &self.build_path, &[&ndk_path], ctx, self)?;

        const CMAKE_GENERATED: &str = "cmake_generated";
        let mut package = SoongPackage::new(
            &["//visibility:public"],
            "clpeak_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[target_typed!("clpeak", "cc_benchmark", "clpeak")]),
            &self.src_path,
            &ndk_path,
            &self.build_path,
            Some(CMAKE_GENERATED),
            self,
            ctx,
        )?;

        let gen_deps = package.get_dep_gen_assets();
        package.filter_gen_deps(CMAKE_GENERATED, &gen_deps)?;
        common::copy_gen_deps(gen_deps, CMAKE_GENERATED, &self.build_path, ctx, self)?;

        package.print(ctx)
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        if target.ends_with("libpeak_opencl.a") {
            module = module
                .extend_prop("extra_include_dir_deps", vec!["src/opencl/kernels/*.cl"])?
                .extend_prop("header_libs", vec!["OpenCL-CLHPP", "OpenCL-Headers"])?;
        } else if target.ends_with("libpeak_vulkan.a") {
            module = module.extend_prop("header_libs", vec!["vulkan_headers"])?;
        } else if target.ends_with("libpeak_litert.a") {
            module = module.extend_prop("header_libs", vec!["gl_headers"])?;
        } else if target.ends_with("clpeak") {
            module = module
                .extend_prop("test_suites", vec!["dts"])?
                .extend_prop(
                    "header_libs",
                    vec!["OpenCL-CLHPP", "OpenCL-Headers", "vulkan_headers"],
                )?;
        }
        module
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

    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_gen_header(&self, _header: &Path) -> bool {
        false
    }
    fn filter_include(&self, include: &Path) -> bool {
        include.starts_with(&self.src_path) || include.starts_with(&self.build_path)
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_lib(&self, lib: &str) -> bool {
        !matches!(lib, "libdl" | "libm" | "libatomic")
    }
}
