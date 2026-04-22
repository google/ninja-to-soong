// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct LlvmProject();

const DEFAULTS: &str = "llvm-project-defaults";
const RAW_DEFAULTS: &str = "llvm-project-raw-defaults";

impl Project for LlvmProject {
    fn get_name(&self) -> &'static str {
        "llvm-project"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external/opencl").join(self.get_name()))
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
            &src_path.join("llvm"),
            &build_path,
            vec![path_to_string(&ndk_path)],
            ctx,
            self,
        )?;

        const CMAKE_GENERATED: &str = "cmake_generated";
        let cmake_generated_path = Path::new(CMAKE_GENERATED);
        let mut package = SoongPackage::new(
            &[],
            "llvm-project_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE.TXT"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&Dep::LlvmProjectTargets.get_ninja_targets(projects_map)?),
            parse_build_ninja::<CmakeNinjaTarget>(&build_path)?,
            &src_path,
            &ndk_path,
            &build_path,
            Some(CMAKE_GENERATED),
            self,
            ctx,
        )?
        .add_visibilities(Dep::ClangHeaders.get_visibilities(projects_map)?)
        .add_visibilities(Dep::LlvmProjectTargets.get_visibilities(projects_map)?)
        .add_module(SoongModule::new_cc_library_headers(
            CcLibraryHeaders::Llvm,
            vec![
                String::from("llvm/include"),
                path_to_string(cmake_generated_path.join("include")),
            ],
        ))
        .add_module(SoongModule::new_cc_library_headers(
            CcLibraryHeaders::Clang,
            vec![
                String::from("clang/include"),
                path_to_string(cmake_generated_path.join("tools/clang/include")),
            ],
        ))
        .add_module(
            SoongModule::new("cc_defaults")
                .add_prop("name", SoongProp::Str(String::from(DEFAULTS)))
                .add_prop(
                    "local_include_dirs",
                    SoongProp::VecStr(vec![
                        String::from(CMAKE_GENERATED) + "/include",
                        String::from("llvm/include"),
                    ]),
                )
                .add_prop(
                    "defaults",
                    SoongProp::VecStr(vec![String::from(RAW_DEFAULTS)]),
                ),
        );

        for clang_header in Dep::ClangHeaders.get(projects_map)? {
            package = package.add_module(SoongModule::new_filegroup(
                Dep::ClangHeaders.get_id(&clang_header, Path::new("clang"), &build_path),
                vec![path_to_string(clang_header)],
            ));
        }

        let mut gen_deps = package.get_dep_gen_assets();
        gen_deps.extend([
            PathBuf::from("include/llvm/Support/VCSRevision.h"),
            PathBuf::from("tools/clang/lib/Basic/VCSVersion.inc"),
        ]);
        common::ninja_build(&build_path, &gen_deps, ctx)?;
        gen_deps.extend(
            [
                "include/llvm/Config/llvm-config.h",
                "include/llvm/Config/abi-breaking.h",
                "include/llvm/Config/config.h",
                "include/llvm/Config/Targets.def",
                "include/llvm/Config/AsmPrinters.def",
                "include/llvm/Config/AsmParsers.def",
                "include/llvm/Config/Disassemblers.def",
                "include/llvm/Config/TargetMCAs.def",
                "include/llvm/Support/Extension.def",
                "tools/clang/include/clang/Basic/Version.inc",
                "tools/clang/include/clang/Config/config.h",
                "tools/clang/tools/driver/clang-driver.cpp",
                "tools/llvm-ar/llvm-ar-driver.cpp",
            ]
            .map(|dep| PathBuf::from(dep)),
        );
        package.filter_gen_deps(CMAKE_GENERATED, &gen_deps)?;
        common::copy_gen_deps(gen_deps, CMAKE_GENERATED, &build_path, ctx, self)?;
        if ctx.copy_to_aosp {
            let llvm_config_h_path = ctx
                .get_android_path(self)?
                .join(CMAKE_GENERATED)
                .join("include/llvm/Config/config.h");
            write_file(
                &llvm_config_h_path,
                &read_file(&llvm_config_h_path)?
                    .replace("#define HAVE_MALLINFO2 1", "//#define HAVE_MALLINFO2 1")
                    .replace(
                        "#define HAVE_DECL_ARC4RANDOM 1",
                        "//#define HAVE_DECL_ARC4RANDOM 1",
                    ),
            )?;
        }

        package
            .add_raw_suffix(&format!(
                r#"
cc_defaults {{
    name: "{RAW_DEFAULTS}",
    optimize_for_size: true,
    vendor_available: true,
    host_supported: true,
    cpp_std: "c++17",
    cflags: [
        "-Wno-error",
        "-Wno-unreachable-code-loop-increment",
    ],
}}
"#
            ))
            .print(ctx)
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        if target.ends_with("libLLVMSupport.a") {
            module = module
                .extend_prop(
                    "cflags",
                    vec![
                        "-DBLAKE3_NO_AVX512",
                        "-DBLAKE3_NO_AVX2",
                        "-DBLAKE3_NO_SSE41",
                        "-DBLAKE3_NO_SSE2",
                    ],
                )?
                .extend_prop("shared_libs", vec!["libz"])?
        }
        Ok(module.add_prop("defaults", SoongProp::VecStr(vec![String::from(DEFAULTS)])))
    }

    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_define(&self, _define: &str) -> bool {
        false
    }
    fn filter_gen_header(&self, _header: &Path) -> bool {
        false
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_target(&self, input: &Path) -> bool {
        (input.starts_with("lib") || input.starts_with("bin"))
            && !file_name(input).ends_with(".inc")
    }
}
