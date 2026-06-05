// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Clvk {
    patched_assets: std::collections::HashMap<String, String>,
    gen_libs: HashMap<Dep, Vec<String>>,
}

impl Clvk {
    fn get_patch_modules_from(
        &mut self,
        dir: PathBuf,
        src_path: &Path,
        patch_root_path: &Path,
    ) -> Result<Vec<SoongModule>, String> {
        let patch_full_path = dir.join("patch");
        if patch_full_path.exists() {
            let patch_string = path_to_string(strip_prefix(patch_full_path, src_path));
            let name = path_to_id(strip_prefix(&dir, src_path.join("android").join("patches")));

            let input_path = path_to_string(strip_prefix(&dir, patch_root_path));
            let input = match self.patched_assets.get(&input_path) {
                Some(input) => Some(format!(":{input}")),
                None => {
                    if src_path.join(&input_path).exists() {
                        Some(path_to_string(&input_path))
                    } else {
                        None
                    }
                }
            };
            self.patched_assets.insert(input_path, name.clone());
            let mut inputs = vec![patch_string.clone()];
            let cmd = match input {
                Some(input) => {
                    inputs.push(input.clone());
                    format!("cp $(location {input}) . && ")
                }
                None => String::new(),
            } + &format!(
                "patch -i $(location {patch_string}) && cp {0} $(out)",
                file_name(&dir)
            );
            return Ok(vec![SoongModule::new("cc_genrule")
                .add_prop("name", SoongProp::Str(name))
                .add_prop("cmd", SoongProp::Str(cmd))
                .add_prop("srcs", SoongProp::VecStr(inputs))
                .add_prop("out", SoongProp::VecStr(vec![file_name(&dir)]))
                .add_prop("soc_specific", SoongProp::Bool(true))]);
        }
        let mut modules = Vec::new();
        for subdir in ls_dir(&dir)? {
            modules.extend(self.get_patch_modules_from(subdir, src_path, patch_root_path)?);
        }
        return Ok(modules);
    }
    fn get_copy_module_for(&mut self, asset: &Path, src_path: &Path) -> Option<SoongModule> {
        let asset = strip_prefix(asset, src_path);
        let asset_str = path_to_string(&asset);
        if !self.patched_assets.contains_key(&asset_str) {
            let name = path_to_id(asset.clone());
            self.patched_assets.insert(asset_str.clone(), name.clone());
            Some(
                SoongModule::new("cc_genrule")
                    .add_prop("name", SoongProp::Str(name))
                    .add_prop("cmd", SoongProp::Str(String::from("cp $(in) $(out)")))
                    .add_prop("srcs", SoongProp::VecStr(vec![asset_str]))
                    .add_prop("out", SoongProp::VecStr(vec![file_name(&asset)]))
                    .add_prop("soc_specific", SoongProp::Bool(true)),
            )
        } else {
            None
        }
    }
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
            vec![
                path_to_string(&ndk_path),
                path_to_string(ProjectId::SpirvHeaders.get_android_path(projects_map, ctx)?),
                path_to_string(ProjectId::SpirvTools.get_android_path(projects_map, ctx)?),
                path_to_string(ProjectId::LlvmProject.get_android_path(projects_map, ctx)?),
                path_to_string(ProjectId::Clspv.get_android_path(projects_map, ctx)?),
            ],
            ctx,
            self,
        )?;

        let mut patch_modules = Vec::new();
        let mut dirs = ls_dir(&src_path.join("android").join("patches"))?;
        if dirs.len() > 0 {
            dirs.sort_unstable();
            for dir in dirs {
                patch_modules.extend(self.get_patch_modules_from(dir.clone(), &src_path, &dir)?);
            }
            for src in ls_regex(&src_path.join("src/*.cpp"))? {
                if let Some(module) = self.get_copy_module_for(&src, &src_path) {
                    patch_modules.push(module);
                }
            }
            for src in ls_regex(&src_path.join("src/*.hpp"))? {
                if let Some(module) = self.get_copy_module_for(&src, &src_path) {
                    patch_modules.push(module);
                }
            }
        }

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

        for module in patch_modules {
            package = package.add_module(module);
        }

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
                        if let Ok(strip) = self
                            .map_lib(lib, LibraryKind::Unspecified)
                            .unwrap()
                            .0
                            .strip_prefix(prefix)
                        {
                            return Some(path_to_string(strip));
                        }
                        None
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
        match self.gen_libs.get(&dep) {
            Some(gen_libs) => gen_libs.iter().map(|lib| target!(lib)).collect(),
            None => Vec::new(),
        }
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        let mut header_libs = vec![String::from("OpenCL-Headers")];
        if target.ends_with("api_tests") {
            header_libs.push(CcLibraryHeaders::SpirvHeaders.str());
            header_libs.push(String::from("vulkan_headers"));
            module = module
                .add_prop(
                    "test_config",
                    SoongProp::Str(String::from("android/api_tests.xml")),
                )
                .extend_prop("cflags", vec!["-Wno-missing-braces"])?;
        } else if target.ends_with("simple_test") {
            module = module.add_prop("gtest", SoongProp::Bool(false)).add_prop(
                "test_config",
                SoongProp::Str(String::from("android/simple_test.xml")),
            );
        } else if target.ends_with("libOpenCL.so") {
            module.update_prop("srcs", |prop| match prop {
                SoongProp::VecStr(srcs) => Ok(SoongProp::VecStr(
                    srcs.into_iter()
                        .filter(|src| !self.patched_assets.contains_key(src))
                        .collect(),
                )),
                _ => Ok(prop),
            })?;
            module = module
                .extend_prop("shared_libs", vec!["libz"])?
                .extend_prop("cflags", vec!["-DCL_ENABLE_BETA_EXTENSIONS"])?
                .add_defaults(CcDefaults::ClspvLlvmDependencies)?
                .add_prop(
                    "generated_sources",
                    SoongProp::VecStr(
                        self.patched_assets
                            .iter()
                            .filter_map(|(asset, module_id)| {
                                if asset.ends_with(".cpp") {
                                    Some(module_id.clone())
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    ),
                )
                .add_prop(
                    "generated_headers",
                    SoongProp::VecStr(
                        self.patched_assets
                            .iter()
                            .filter_map(|(asset, module_id)| {
                                if !asset.ends_with(".cpp") {
                                    Some(module_id.clone())
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    ),
                );
        }
        Ok(module
            .add_prop("soc_specific", SoongProp::Bool(true))
            .add_prop("header_libs", SoongProp::VecStr(header_libs)))
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
