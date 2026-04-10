// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0;

use crate::context::*;
use crate::ninja_target::*;
use crate::project::*;
use crate::soong_module::*;
use crate::soong_module_generator::*;
use crate::utils::*;

#[derive(Default)]
pub struct SoongPackage {
    modules: Vec<SoongModule>,
    internals: SoongModuleGeneratorInternals,
    raw_suffix: String,
    raw_prefix: String,
    license_module_name: String,
    visibilities: Vec<String>,
}

impl SoongPackage {
    pub fn new(
        default_visibility: &[&str],
        license_module_name: &str,
        license_kinds: &[&str],
        license_text: &[&str],
    ) -> Self {
        let mut package = Self::default();
        let license_module_name = String::from(license_module_name);
        package.license_module_name = license_module_name.clone();
        package.visibilities = default_visibility
            .into_iter()
            .map(|visibility| String::from(*visibility))
            .collect();
        package.add_module(
            SoongModule::new("license")
                .add_prop("name", SoongProp::Str(license_module_name))
                .add_prop(
                    "visibility",
                    SoongProp::VecStr(vec![String::from(":__subpackages__")]),
                )
                .add_prop(
                    "license_kinds",
                    SoongProp::VecStr(
                        license_kinds
                            .into_iter()
                            .map(|kind| String::from(*kind))
                            .collect(),
                    ),
                )
                .add_prop(
                    "license_text",
                    SoongProp::VecStr(
                        license_text
                            .into_iter()
                            .map(|text| String::from(*text))
                            .collect(),
                    ),
                ),
        )
    }

    pub fn add_visibilities(mut self, visibilities: Vec<String>) -> SoongPackage {
        self.visibilities.extend(visibilities);
        self
    }

    pub fn add_module(mut self, module: SoongModule) -> SoongPackage {
        self.modules.push(module);
        self
    }

    pub fn add_raw_suffix(mut self, suffix: &str) -> SoongPackage {
        self.raw_suffix = String::from(suffix);
        self
    }

    pub fn add_raw_prefix(mut self, prefix: &str) -> SoongPackage {
        self.raw_prefix = String::from(prefix);
        self
    }

    pub fn filter_gen_deps(&mut self, prefix: &str, files: &Vec<PathBuf>) -> Result<(), String> {
        let mut set = std::collections::HashSet::new();
        for file in files {
            let mut path = file.clone();
            set.insert(path.clone());
            while let Some(parent) = path.parent() {
                path = PathBuf::from(parent);
                set.insert(path.clone());
            }
        }
        let filter = |prop| {
            let SoongProp::VecStr(dirs) = prop else {
                return Ok(prop);
            };
            Ok(SoongProp::VecStr(
                dirs.into_iter()
                    .filter(|dir| {
                        if let Ok(strip) = Path::new(&dir).strip_prefix(prefix) {
                            if !set.contains(strip) {
                                return false;
                            }
                        }
                        return true;
                    })
                    .collect(),
            ))
        };
        for module in &mut self.modules {
            module.update_prop("local_include_dirs", filter)?;
            module.update_prop("srcs", filter)?;
        }
        Ok(())
    }

    fn filter_default(&self, mut module: SoongModule) -> Result<SoongModule, String> {
        let Some(default_names_prop) = module.get_prop("defaults") else {
            return Ok(module);
        };
        let SoongProp::VecStr(default_names) = default_names_prop.get_prop() else {
            return error!("Unexpected property");
        };
        for default_name in default_names {
            let Some(default_module) = self.get_module(&default_name) else {
                return Ok(module);
            };
            module.filter_default(default_module)?;
        }
        Ok(module)
    }

    pub fn get_props(
        &self,
        module_name: &str,
        props: Vec<&str>,
    ) -> Result<Vec<SoongNamedProp>, String> {
        let Some(module) = self.get_module(module_name) else {
            return error!("Could not find module {module_name:#?}");
        };
        Ok(props
            .into_iter()
            .filter_map(|prop| module.get_prop(prop))
            .collect())
    }

    fn get_module(&self, name: &str) -> Option<&SoongModule> {
        for module in &self.modules {
            let Some(module_name_prop) = module.get_prop("name") else {
                continue;
            };
            let SoongProp::Str(module_name) = module_name_prop.get_prop() else {
                continue;
            };
            if module_name == name {
                return Some(module);
            }
        }
        None
    }

    pub fn pop_module(&mut self, name: &str) -> Option<SoongModule> {
        for idx in 0..self.modules.len() {
            let module = &self.modules[idx];
            let Some(module_name_prop) = module.get_prop("name") else {
                continue;
            };
            let SoongProp::Str(module_name) = module_name_prop.get_prop() else {
                continue;
            };
            if module_name == name {
                return Some(self.modules.remove(idx));
            }
        }
        None
    }

    pub fn get_modules_name(&self) -> Vec<String> {
        self.modules
            .iter()
            .filter_map(|module| {
                let Some(prop) = module.get_prop("name") else {
                    return None;
                };
                match prop.get_prop() {
                    SoongProp::Bool(_) => None,
                    SoongProp::Str(name) => Some(name),
                    SoongProp::Prop(_) => None,
                    SoongProp::VecStr(_) => None,
                    SoongProp::None => None,
                }
            })
            .collect()
    }

    pub fn print(mut self, ctx: &Context) -> Result<String, String> {
        let mut package = String::from(
            "//
// This file has been auto-generated by ninja-to-soong
//
// ******************************
// *** DO NOT MODIFY MANUALLY ***
// ******************************
//
// https://github.com/google/ninja-to-soong
//
",
        );
        if !ctx.wildcardize_paths {
            package += "// CI version, no wildcard generated\n";
            package += "//\n";
        }
        package += &self.raw_prefix;
        for module_index in 0..self.modules.len() {
            let module = self.modules.remove(module_index);
            self.modules
                .insert(module_index, self.filter_default(module)?);
        }
        self.visibilities.sort_unstable();
        self.visibilities.dedup();
        package += &SoongModule::new("package")
            .add_prop("default_visibility", SoongProp::VecStr(self.visibilities))
            .add_prop(
                "default_applicable_licenses",
                SoongProp::VecStr(vec![self.license_module_name]),
            )
            .print();
        for module in self.modules {
            package += &module.print();
        }
        package += &self.raw_suffix;
        Ok(package)
    }

    pub fn get_dep_gen_assets(&mut self) -> Vec<PathBuf> {
        self.internals.gen_assets.sort_unstable();
        self.internals.gen_assets.dedup();
        std::mem::take(&mut self.internals.gen_assets)
    }

    pub fn get_dep_libs(&mut self) -> Vec<PathBuf> {
        self.internals.libs.sort_unstable();
        self.internals.libs.dedup();
        std::mem::take(&mut self.internals.libs)
    }

    pub fn get_dep_custom_cmd_inputs(&mut self) -> Vec<PathBuf> {
        self.internals.custom_cmd_inputs.sort_unstable();
        self.internals.custom_cmd_inputs.dedup();
        std::mem::take(&mut self.internals.custom_cmd_inputs)
    }

    pub fn get_dep_tools_module(&mut self) -> Vec<PathBuf> {
        self.internals.tools_module.sort_unstable();
        self.internals.tools_module.dedup();
        std::mem::take(&mut self.internals.tools_module)
    }

    pub fn generate_from_map<T>(
        mut self,
        targets_to_gen: NinjaTargetsToGenMap,
        targets_map: NinjaTargetsMap<T>,
        src_path: &Path,
        ndk_path: &Path,
        build_path: &Path,
        gen_build_prefix: Option<&str>,
        project: &dyn Project,
        ctx: &Context,
    ) -> Result<SoongPackage, String>
    where
        T: NinjaTarget,
    {
        let mut gen = SoongModuleGenerator::new(
            src_path,
            ndk_path,
            build_path,
            gen_build_prefix,
            &targets_map,
            &targets_to_gen,
            project,
        );
        targets_map.traverse_from(targets_to_gen.get_targets(), false, |target| {
            if !gen.filter_target(target) {
                return Ok(false);
            }
            self.modules.extend(match target.get_rule()? {
                NinjaRule::Binary => gen.generate_object("cc_binary", target, ctx)?,
                NinjaRule::SharedLibrary => {
                    gen.generate_object("cc_library_shared", target, ctx)?
                }
                NinjaRule::StaticLibrary => {
                    gen.generate_object("cc_library_static", target, ctx)?
                }
                NinjaRule::CustomCommand(rule_cmd) => {
                    gen.generate_custom_command(target, rule_cmd)?
                }
                NinjaRule::None => return Ok(true),
            });
            Ok(true)
        })?;
        self.internals = gen.delete();

        Ok(self)
    }

    pub fn generate<T>(
        self,
        targets_to_gen: NinjaTargetsToGenMap,
        targets: Vec<T>,
        src_path: &Path,
        ndk_path: &Path,
        build_path: &Path,
        gen_build_prefix: Option<&str>,
        project: &dyn Project,
        ctx: &Context,
    ) -> Result<SoongPackage, String>
    where
        T: NinjaTarget,
    {
        let targets_map = NinjaTargetsMap::new(&targets);
        self.generate_from_map(
            targets_to_gen,
            targets_map,
            src_path,
            ndk_path,
            build_path,
            gen_build_prefix,
            project,
            ctx,
        )
    }
}
