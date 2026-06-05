// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Clspv {
    build_path: PathBuf,
    spirv_headers_path: PathBuf,
    llvm_project_path: PathBuf,
    gen_deps: HashMap<Dep, Vec<String>>,
}

impl Project for Clspv {
    fn get_name(&self) -> &'static str {
        "clspv"
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
        self.build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;
        self.spirv_headers_path = ProjectId::SpirvHeaders.get_android_path(projects_map, ctx)?;
        self.llvm_project_path = ProjectId::LlvmProject.get_android_path(projects_map, ctx)?;

        common::gen_ninja(
            &src_path,
            &self.build_path,
            vec![
                path_to_string(&ndk_path),
                path_to_string(&self.spirv_headers_path),
                path_to_string(ProjectId::SpirvTools.get_android_path(projects_map, ctx)?),
                path_to_string(&self.llvm_project_path),
            ],
            ctx,
            self,
        )?;

        let mut package = SoongPackage::new(
            &[],
            "clspv_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&Dep::ClspvTargets.get_ninja_targets(projects_map)?),
            parse_build_ninja::<CmakeNinjaTarget>(&self.build_path)?,
            &src_path,
            &ndk_path,
            &self.build_path,
            None,
            self,
            ctx,
        )?
        .add_visibilities(Dep::ClspvTargets.get_visibilities(projects_map)?);

        package = package.add_module(
            SoongModule::new_cc_defaults(CcDefaults::ClspvLlvmDependencies).add_prop(
                "static_libs",
                SoongProp::VecStr(
                    Dep::LlvmProjectTargets
                        .get(projects_map)?
                        .into_iter()
                        .map(|lib| path_to_id(Path::new(common::LLVM_PROJECT_NAME).join(lib)))
                        .collect(),
                ),
            ),
        );

        let gen_deps = package.get_dep_custom_cmd_inputs();
        self.gen_deps.insert(
            Dep::ClangHeaders,
            gen_deps
                .iter()
                .filter_map(|dep| {
                    if let Ok(strip) = dep.strip_prefix(&self.llvm_project_path) {
                        return Some(path_to_string(strip));
                    }
                    None
                })
                .collect(),
        );
        self.gen_deps.insert(
            Dep::LibclcBins,
            gen_deps
                .iter()
                .filter_map(|dep| {
                    let file_name = file_name(dep);
                    if file_name == "libclc.bc" {
                        return Some(path_to_string(strip_prefix(dep, "/libclc")));
                    }
                    None
                })
                .collect(),
        );
        self.gen_deps.insert(
            Dep::SpirvHeaders,
            gen_deps
                .iter()
                .filter_map(|dep| {
                    if let Ok(strip) = dep.strip_prefix(&self.spirv_headers_path) {
                        return Some(path_to_string(strip));
                    }
                    None
                })
                .collect(),
        );

        package.print(ctx)
    }

    fn get_deps(&self, dep: Dep) -> Vec<NinjaTargetToGen> {
        match self.gen_deps.get(&dep) {
            Some(gen_deps) => gen_deps.iter().map(|lib| target!(lib)).collect(),
            None => Vec::new(),
        }
    }

    fn extend_module(&self, _target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        module
            .add_prop("optimize_for_size", SoongProp::Bool(true))
            .add_prop("vendor_available", SoongProp::Bool(true))
            .add_prop(
                "header_libs",
                SoongProp::VecStr(vec![
                    CcLibraryHeaders::SpirvHeaders.str(),
                    CcLibraryHeaders::Llvm.str(),
                    CcLibraryHeaders::Clang.str(),
                ]),
            )
            .extend_prop("export_include_dirs", vec!["include"])
    }
    fn extend_custom_command(
        &self,
        _target: &Path,
        module: SoongModule,
    ) -> Result<SoongModule, String> {
        Ok(module.add_prop("vendor_available", SoongProp::Bool(true)))
    }

    fn map_cmd_input(&self, input: &Path) -> Option<String> {
        for (prefix, dep) in [
            (&self.spirv_headers_path, Dep::SpirvHeaders),
            (&self.llvm_project_path.join("clang"), Dep::ClangHeaders),
        ] {
            if input.starts_with(&prefix) {
                return Some(dep.get_id(&input, prefix, &self.build_path));
            }
        }
        if let Ok(prefix) = input.strip_prefix(PathBuf::from("/libclc")) {
            return Some(path_to_id(PathBuf::from("libclc").join(prefix)));
        }
        None
    }
    fn map_cmd_output(&self, output: &Path) -> Option<String> {
        let mut prefix = output;
        while let Some(parent) = prefix.parent() {
            prefix = parent;
            if file_name(prefix) == "include" {
                return Some(path_to_string(strip_prefix(output, prefix)));
            }
        }
        Some(path_to_string(output))
    }

    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_define(&self, _define: &str) -> bool {
        false
    }
    fn filter_gen_header(&self, header: &Path) -> bool {
        !header.starts_with("third_party/llvm")
    }
    fn filter_include(&self, include: &Path) -> bool {
        !(include.starts_with(&self.build_path)
            || include.starts_with(&self.spirv_headers_path)
            || include.starts_with(&self.llvm_project_path))
    }
    fn filter_target(&self, target: &Path) -> bool {
        !target.starts_with("third_party/")
    }
}
