// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::str;

const DEFAULTS: &str = "libclc-defaults";

#[derive(Default)]
pub struct LibCLC {
    src_path: PathBuf,
    host_tools: Vec<String>,
    opencl_c_base: String,
}

impl LibCLC {
    fn generate_package_for(
        &mut self,
        target: NinjaTargetToGen,
        ctx: &Context,
    ) -> Result<SoongPackage, String> {
        let target_name = String::from(
            match path_to_string(Path::new(&target.path).parent().unwrap()).as_str() {
                "spir--" => "clspv--",
                "spir64--" => "clspv64--",
                _ => return error!("Unsupported target '{target:#?}'"),
            },
        );
        let build_path = ctx.get_temp_path(&Path::new(self.get_name()).join(&target_name))?;
        common::gen_ninja(&self.src_path, &build_path, vec![target_name], ctx, self)?;
        let mut package = SoongPackage::default().generate(
            NinjaTargetsToGenMap::from(&[target]),
            parse_build_ninja::<CmakeNinjaTarget>(&build_path)?,
            &self.src_path,
            Path::new("<no_sdk>"),
            &build_path,
            None,
            self,
            ctx,
        )?;
        self.host_tools.extend(
            package
                .get_dep_tools_module()
                .into_iter()
                .map(|tool| path_to_string(strip_prefix(tool, "llvm-project"))),
        );

        Ok(package)
    }
}

impl Project for LibCLC {
    fn get_name(&self) -> &'static str {
        "libclc"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external/opencl/llvm-project").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        self.src_path = ctx.get_android_path(self)?;

        self.opencl_c_base = String::from(":")
            + &Dep::ClangHeaders.get_id(
                Path::new("lib/Headers/opencl-c-base.h"),
                Path::new(""),
                Path::new(""),
            );

        let mut package = SoongPackage::new(
            &[],
            "llvm-project_libclc_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE.TXT"],
        );
        for target in Dep::LibclcBins.get_ninja_targets(projects_map)? {
            let mut sub_package = self.generate_package_for(target, ctx)?;
            for module_name in sub_package.get_modules_name() {
                package = package.add_module(sub_package.pop_module(&module_name).unwrap());
            }
        }
        package
            .add_visibilities(Dep::LibclcBins.get_visibilities(projects_map)?)
            .add_raw_suffix(&format!(
                r#"
cc_genrule_defaults {{
    name: "{DEFAULTS}",
    srcs: [
        "CMakeLists.txt",
        "opencl/include/*.h",
        "opencl/include/**/*.h",
        "opencl/include/**/*.inc",
        "opencl/lib/generic/*.h",
        "opencl/lib/generic/**/*.h",
        "opencl/lib/generic/**/*.inc",
        "opencl/lib/clspv/**/*.inc",
        "clc/include/clc/*.h",
        "clc/include/clc/**/*.h",
        "clc/include/clc/**/*.inc",
        "clc/lib/generic/**/*.h",
        "clc/lib/generic/**/*.inc",
        "{0}",
    ],
    vendor_available: true,
}}
"#,
                self.opencl_c_base,
            ))
            .print(ctx)
    }

    fn get_deps(&self, _dep: Dep) -> Vec<NinjaTargetToGen> {
        self.host_tools
            .iter()
            .map(|host_tool| target!(host_tool.clone()))
            .collect()
    }

    fn extend_module(&self, _target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        Ok(module.add_prop("vendor_available", SoongProp::Bool(true)))
    }
    fn extend_custom_command(
        &self,
        _target: &Path,
        mut module: SoongModule,
    ) -> Result<SoongModule, String> {
        let Some(cmd_prop) = module.get_prop("cmd") else {
            return Ok(module);
        };
        let SoongProp::Str(mut cmd) = cmd_prop.get_prop() else {
            return Ok(module);
        };
        let src_path = path_to_string(&self.src_path);
        let include = String::from("-I") + &src_path;
        while let Some(begin) = cmd.find(&include) {
            let include = match str::from_utf8(&cmd.as_bytes()[begin..]).unwrap().find(" ") {
                Some(end) => str::from_utf8(&cmd.as_bytes()[(begin + 2)..(begin + end)]).unwrap(),
                None => str::from_utf8(&cmd.as_bytes()[(begin + 2)..]).unwrap(),
            };
            let local_include = strip_prefix(include, &self.src_path);
            cmd = cmd.replace(
                include,
                &(String::from("$$(dirname $(location CMakeLists.txt))/")
                    + &path_to_string(&local_include)),
            );
        }

        cmd = cmd.replace(
            "opencl-c-base.h",
            &(String::from("$(location ") + &self.opencl_c_base + ")"),
        );

        module.update_prop("cmd", |_| Ok(SoongProp::Str(cmd.clone())))?;
        Ok(module.add_prop("defaults", SoongProp::VecStr(vec![String::from(DEFAULTS)])))
    }

    fn map_tool_module(&self, tool_module: &Path) -> Option<PathBuf> {
        let tool_module = path_to_string(tool_module);
        Some(PathBuf::from(if tool_module.contains("clang") {
            "llvm-project/bin/clang-23"
        } else if tool_module.contains("llvm-link") {
            "llvm-project/bin/llvm-link"
        } else if tool_module.contains("opt") {
            "llvm-project/bin/opt"
        } else if tool_module.contains("llvm-ar") {
            "llvm-project/bin/llvm-ar"
        } else {
            return None;
        }))
    }
}
