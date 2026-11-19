// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Clvk {
    gen_libs: HashMap<Dep, Vec<String>>,
}

impl Project for Clvk {
    fn get_name(&self) -> &'static str {
        "clvk"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        let src_path = ctx.get_android_path(self)?;
        let build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;

        common::gen_ninja(
            &src_path,
            &build_path,
            &[
                ndk_path.clone(),
                ProjectId::SpirvHeaders.get_android_path(projects_map, ctx)?,
                ProjectId::SpirvTools.get_android_path(projects_map, ctx)?,
                ProjectId::LlvmProject.get_android_path(projects_map, ctx)?,
                ProjectId::Clspv.get_android_path(projects_map, ctx)?,
            ],
            ctx,
            self,
        )?;

        const LIBCLVK: &str = "libclvk";
        let mut package = SoongPackage::new(
            &[],
            "clvk_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[
                target!("libOpenCL.so", LIBCLVK),
                target_typed!("simple_test", "cc_test"),
                target_typed!("api_tests", "cc_test"),
            ]),
            parse_build_ninja::<CmakeNinjaTarget>(&build_path)?,
            &src_path,
            &ndk_path,
            &build_path,
            None,
            self,
            ctx,
        )?;

        let gen_libs = package.get_dep_libs();
        for (dep, prefix) in [
            (Dep::ClspvTargets, "clspv"),
            (Dep::LlvmProjectTargets, common::LLVM_PROJECT_NAME),
            (Dep::SpirvToolsTargets, "SPIRV-Tools"),
        ] {
            self.gen_libs.insert(
                dep,
                gen_libs
                    .iter()
                    .filter_map(|lib| {
                        self.map_lib(lib, LibraryKind::Unspecified)
                            .unwrap()
                            .0
                            .strip_prefix(prefix)
                            .ok()
                            .map(path_to_string)
                    })
                    .collect(),
            );
        }

        const CLVK_ICD_GENRULE: &str = "clvk_icd_genrule";
        package
            .add_raw_suffix(&format!(
                r#"
cc_genrule {{
    name: "{CLVK_ICD_GENRULE}",
    cmd: "echo /vendor/$$CC_MULTILIB/{LIBCLVK}.so > $(out)",
    out: ["clvk.icd"],
    soc_specific: true,
}}

prebuilt_etc {{
    name: "clvk_icd_prebuilt",
    src: ":{CLVK_ICD_GENRULE}",
    filename_from_src: true,
    relative_install_path: "Khronos/OpenCL/vendors",
    soc_specific: true,
}}
"#
            ))
            .print(ctx)
    }

    fn get_deps(&self, dep: Dep) -> Vec<NinjaTargetToGen> {
        self.gen_libs
            .get(&dep)
            .into_iter()
            .flatten()
            .map(|lib| target!(lib))
            .collect()
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        let mut header_libs = vec![String::from("OpenCL-Headers")];
        if target.ends_with("api_tests") {
            header_libs.push(CcLibraryHeaders::SpirvHeaders.str());
            header_libs.push(String::from("vulkan_headers"));
            module = module
                .add_prop("test_config", "android/api_tests.xml")
                .extend_prop("cflags", vec!["-Wno-missing-braces"])?;
        } else if target.ends_with("simple_test") {
            module = module
                .add_prop("gtest", false)
                .add_prop("test_config", "android/simple_test.xml");
        } else if target.ends_with("libOpenCL.so") {
            module = module
                .extend_prop("shared_libs", vec!["libz"])?
                .add_defaults(CcDefaults::ClspvLlvmDependencies)?;
        }
        Ok(module
            .add_prop("soc_specific", true)
            .add_prop("header_libs", header_libs))
    }

    fn map_lib(&self, library: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        if let Ok(strip) = library.strip_prefix(Path::new("external/clspv/third_party/llvm")) {
            return Some((
                Path::new(common::LLVM_PROJECT_NAME).join(strip),
                LibraryKind::Unspecified,
            ));
        }
        Some((strip_prefix(PathBuf::from(library), "external"), kind))
    }

    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_gen_header(&self, _header: &Path) -> bool {
        false
    }
    fn filter_include(&self, include: &Path) -> bool {
        include.ends_with("src")
    }
    fn filter_lib(&self, lib: &str) -> bool {
        !lib.contains("gtest")
    }
    fn filter_link_flag(&self, flag: &str) -> bool {
        flag == "-Wl,-Bsymbolic"
    }
    fn filter_target(&self, target: &Path) -> bool {
        !target.starts_with("external")
    }
}
