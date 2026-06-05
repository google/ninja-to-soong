// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::str;

use crate::context::*;
use crate::ninja_target::common::*;
use crate::ninja_target::*;
use crate::project::*;
use crate::soong_module::*;
use crate::utils::*;

#[derive(Default)]
pub struct SoongModuleGeneratorInternals {
    pub gen_assets: Vec<PathBuf>,
    pub libs: Vec<PathBuf>,
    pub custom_cmd_inputs: Vec<PathBuf>,
    pub tools_module: Vec<PathBuf>,
    python_binaries: std::collections::HashSet<String>,
    python_libraries: std::collections::HashSet<String>,
}

pub struct SoongModuleGenerator<'a, T>
where
    T: NinjaTarget,
{
    internals: SoongModuleGeneratorInternals,
    src_path: &'a Path,
    ndk_path: &'a Path,
    build_path: &'a Path,
    gen_build_prefix: Option<&'a str>,
    targets_map: &'a NinjaTargetsMap<'a, T>,
    targets_to_gen: &'a NinjaTargetsToGenMap,
    project: &'a dyn Project,
}

impl<'a, T> SoongModuleGenerator<'a, T>
where
    T: NinjaTarget,
{
    pub fn new(
        src_path: &'a Path,
        ndk_path: &'a Path,
        build_path: &'a Path,
        gen_build_prefix: Option<&'a str>,
        targets_map: &'a NinjaTargetsMap<'a, T>,
        targets_to_gen: &'a NinjaTargetsToGenMap,
        project: &'a dyn Project,
    ) -> Self {
        Self {
            internals: SoongModuleGeneratorInternals::default(),
            src_path,
            ndk_path,
            build_path,
            gen_build_prefix,
            targets_map,
            targets_to_gen,
            project,
        }
    }
    pub fn delete(self) -> SoongModuleGeneratorInternals {
        self.internals
    }

    pub fn filter_target(&self, target: &T) -> bool {
        let target_name = target.get_name();
        debug_project!("filter_target({target_name:#?})");
        self.project.filter_target(&target_name)
    }

    fn replace_path(&self, iter: impl Iterator<Item = String>) -> Vec<String> {
        let iter = iter.map(|path| {
            path.replace(&path_to_string_with_separator(self.src_path), "")
                .replace(&path_to_string(self.src_path), "")
        });
        if let Some(prefix) = self.gen_build_prefix {
            iter.map(|path| path.replace(&path_to_string(self.build_path), prefix))
                .collect()
        } else {
            iter.collect()
        }
    }
    fn get_sources(&self, sources: Vec<PathBuf>) -> Vec<String> {
        self.replace_path(sources.iter().filter_map(|source| {
            debug_project!("filter_source({source:#?})");
            if !self.project.filter_source(source) {
                return None;
            }
            Some(path_to_string(source))
        }))
    }
    fn get_defines(&self, defines: Vec<String>) -> Vec<String> {
        self.replace_path(defines.into_iter().filter(|def| {
            debug_project!("filter_define({def})");
            self.project.filter_define(&def)
        }))
        .iter()
        .map(|def| format!("-D{def}"))
        .collect()
    }
    fn get_cflags(&self, cflags: Vec<String>) -> Vec<String> {
        cflags
            .into_iter()
            .filter(|cflag| {
                debug_project!("filter_cflags({cflag})");
                self.project.filter_cflag(cflag)
            })
            .collect()
    }
    fn get_includes(&self, includes: Vec<PathBuf>) -> Vec<String> {
        self.replace_path(includes.iter().filter_map(|include| {
            debug_project!("filter_include({include:#?})");
            if !self.project.filter_include(include) {
                return None;
            }
            Some(path_to_string(include))
        }))
    }
    fn get_libs(
        &mut self,
        libs: Vec<PathBuf>,
        module_name: &String,
        kind: LibraryKind,
    ) -> Vec<(String, LibraryKind)> {
        libs.into_iter()
            .filter_map(|lib| {
                debug_project!("filter_lib({lib:#?})");
                if !self.project.filter_lib(&path_to_string(&lib)) {
                    return None;
                }
                Some(if lib.starts_with(&self.ndk_path) {
                    (file_stem(&lib), kind)
                } else {
                    let (lib_path, lib_kind) = match self.project.map_lib(&lib, kind) {
                        Some((map_lib, lib_kind)) => match self.targets_to_gen.get_name(&map_lib) {
                            Some(name) => (name, lib_kind),
                            None => (map_lib, lib_kind),
                        },
                        None => match self.targets_to_gen.get_name(&lib) {
                            Some(name) => (name, kind),
                            None => (Path::new(self.project.get_name()).join(&lib), kind),
                        },
                    };
                    let lib_id = path_to_id(lib_path);
                    if lib_id == *module_name {
                        return None;
                    }
                    self.internals.libs.push(lib);
                    (lib_id, lib_kind)
                })
            })
            .collect()
    }
    fn get_link_flags(&self, link_flags: Vec<String>) -> Vec<String> {
        link_flags
            .into_iter()
            .filter(|flag| {
                debug_project!("filter_link_flag({flag})");
                self.project.filter_link_flag(flag)
            })
            .collect()
    }
    fn get_generated_assets(
        &mut self,
        target: &T,
        filter_header: bool,
    ) -> Result<Vec<String>, String> {
        let mut gen_assets = Vec::new();
        self.targets_map
            .traverse_from(target.get_outputs().clone(), !filter_header, |target| {
                if target.get_outputs().len() > 1 {
                    return Ok(false);
                }
                if let NinjaRule::CustomCommand(_) = target.get_rule()? {
                    gen_assets.extend(target.get_outputs().clone())
                }
                Ok(true)
            })?;
        Ok(gen_assets
            .iter()
            .filter_map(|asset| {
                if file_ext(asset).starts_with(if filter_header { "c" } else { "h" }) {
                    return None;
                }
                debug_project!(
                    "filter_gen_{0}({asset:#?})",
                    if filter_header { "header" } else { "source" }
                );
                if (filter_header && !self.project.filter_gen_header(asset))
                    || (!filter_header && !self.project.filter_gen_source(asset))
                {
                    self.internals.gen_assets.push(PathBuf::from(asset));
                    return None;
                }
                let Some(target) = self.targets_map.get(asset) else {
                    self.internals.gen_assets.push(PathBuf::from(asset));
                    return None;
                };
                Some(match self.targets_to_gen.get_name(asset) {
                    Some(name) => path_to_string(name),
                    None => path_to_id(Path::new(self.project.get_name()).join(target.get_name())),
                })
            })
            .collect())
    }
    fn get_generated_headers(&mut self, target: &T) -> Result<Vec<String>, String> {
        self.get_generated_assets(target, true)
    }
    fn get_generated_sources(&mut self, target: &T) -> Result<Vec<String>, String> {
        self.get_generated_assets(target, false)
    }
    fn defines_conflict(
        defines: &mut std::collections::HashMap<String, String>,
        cflags: &Vec<String>,
    ) -> bool {
        let new_defines = cflags
            .iter()
            .filter_map(|cflag| {
                let Some(def) = cflag.strip_prefix("-D") else {
                    return None;
                };
                let Some((var, val)) = def.split_once("=") else {
                    return None;
                };
                Some((String::from(var), String::from(val)))
            })
            .collect::<Vec<_>>();
        if new_defines.iter().any(|(var, val)| {
            let Some(ref_val) = defines.get(var) else {
                return false;
            };
            val != ref_val
        }) {
            return true;
        }
        defines.extend(new_defines);
        false
    }
    pub fn generate_object(
        &mut self,
        module_type: &str,
        target: &T,
        ctx: &Context,
    ) -> Result<Vec<SoongModule>, String> {
        let target_name = target.get_name();
        let module_name = path_to_id(match self.targets_to_gen.get_name(&target_name) {
            Some(name) => name,
            None => Path::new(self.project.get_name()).join(&target_name),
        });
        let mut modules = Vec::new();
        let mut cflags = Vec::new();
        let mut includes = Vec::new();
        let mut sources = Vec::new();
        let mut libs = Vec::new();
        let mut whole_static_libs = Vec::new();
        let mut defines = std::collections::HashMap::new();
        for input in target.get_inputs() {
            let Some(input_target) = self.targets_map.get(input) else {
                sources.push(path_to_string(strip_prefix(
                    canonicalize_path(input, self.build_path),
                    self.src_path,
                )));
                continue;
            };

            let mut input_cflags = self.get_defines(input_target.get_defines());
            input_cflags.extend(self.get_cflags(input_target.get_cflags()));
            if !Self::defines_conflict(&mut defines, &input_cflags) {
                libs.extend(self.get_libs(
                    input_target.get_libs_static_whole(),
                    &module_name,
                    LibraryKind::StaticWhole,
                ));
                libs.extend(self.get_libs(
                    input_target.get_libs_static(),
                    &module_name,
                    LibraryKind::Static,
                ));
                libs.extend(self.get_libs(
                    input_target.get_libs_shared(),
                    &module_name,
                    LibraryKind::Shared,
                ));
                sources.extend(self.get_sources(input_target.get_sources(self.build_path)?));
                includes.extend(self.get_includes(input_target.get_includes(self.build_path)));
                cflags.extend(input_cflags);
            } else {
                modules.extend(self.generate_object("cc_library_static", input_target, ctx)?);
                whole_static_libs.push(path_to_id(Path::new(self.project.get_name()).join(input)));
                continue;
            }
        }
        includes.extend(self.get_includes(target.get_includes(self.build_path)));
        cflags.extend(self.get_defines(target.get_defines()));
        cflags.extend(self.get_cflags(target.get_cflags()));

        let generated_headers = self.get_generated_headers(target)?;
        let generated_sources = self.get_generated_sources(target)?;
        let (version_script, link_flags) = target.get_link_flags();
        let link_flags = self.get_link_flags(link_flags);
        libs.extend(self.get_libs(
            target.get_libs_static_whole(),
            &module_name,
            LibraryKind::StaticWhole,
        ));
        libs.extend(self.get_libs(target.get_libs_static(), &module_name, LibraryKind::Static));
        libs.extend(self.get_libs(target.get_libs_shared(), &module_name, LibraryKind::Shared));
        whole_static_libs.extend(libs.iter().filter_map(|(lib, kind)| {
            if *kind != LibraryKind::StaticWhole {
                return None;
            }
            Some(lib.clone())
        }));
        let static_libs = libs
            .iter()
            .filter_map(|(lib, kind)| {
                if *kind != LibraryKind::Static {
                    return None;
                }
                Some(lib.clone())
            })
            .collect();
        let shared_libs = libs
            .iter()
            .filter_map(|(lib, kind)| {
                if *kind != LibraryKind::Shared {
                    return None;
                }
                Some(lib.clone())
            })
            .collect();

        let module_type = match self.targets_to_gen.get_module_name(&target_name) {
            Some(module_type) => module_type,
            None => String::from(module_type),
        };
        let mut module =
            SoongModule::new(&module_type).add_prop("name", SoongProp::Str(module_name));
        if let Some(stem) = self.targets_to_gen.get_stem(&target_name) {
            module = module.add_prop("stem", SoongProp::Str(stem));
        }
        if let Some(vs) = version_script {
            module = module.add_prop(
                "version_script",
                SoongProp::Str(path_to_string(strip_prefix(vs, &self.src_path))),
            );
        }
        let mut srcs_prop = SoongNamedProp::new("srcs", SoongProp::VecStr(sources));
        if ctx.wildcardize_paths {
            srcs_prop.enable_wildcard(&self.src_path)?;
        }
        module = module
            .add_named_prop(srcs_prop)
            .add_prop("cflags", SoongProp::VecStr(cflags))
            .add_prop("ldflags", SoongProp::VecStr(link_flags))
            .add_prop("shared_libs", SoongProp::VecStr(shared_libs))
            .add_prop("static_libs", SoongProp::VecStr(static_libs))
            .add_prop("whole_static_libs", SoongProp::VecStr(whole_static_libs))
            .add_prop("local_include_dirs", SoongProp::VecStr(includes))
            .add_prop("generated_sources", SoongProp::VecStr(generated_sources))
            .add_prop("generated_headers", SoongProp::VecStr(generated_headers));

        modules.push(self.project.extend_module(&target_name, module)?);
        Ok(modules)
    }

    fn map_cmd_output(&self, output: &Path) -> String {
        if output.starts_with("n2s") {
            path_to_string(output)
        } else if let Some(output) = self.project.map_cmd_output(output) {
            output
        } else {
            path_to_string(output)
        }
    }
    fn replace_in_splitted_cmd(
        &self,
        cmd: &str,
        inputs: &Vec<PathBuf>,
        outputs: &Vec<PathBuf>,
        deps: &Vec<(PathBuf, String)>,
    ) -> String {
        let cmd = String::from(cmd);
        if cmd.is_empty() {
            return cmd;
        }
        for input in inputs {
            let canonicalize_input = path_to_string(canonicalize_path(input, self.build_path));
            let input_string = path_to_string(&input);
            let froms = if input_string.contains(&canonicalize_input) {
                vec![input_string, canonicalize_input]
            } else {
                vec![canonicalize_input, input_string]
            };
            if froms.contains(&cmd) {
                return format!(
                    "$(location {0})",
                    path_to_string(strip_prefix(
                        canonicalize_path(input, self.build_path),
                        self.src_path,
                    ))
                );
            }
        }
        for (dep, dep_target_name) in deps {
            if vec![
                path_to_string(canonicalize_path(&dep, self.build_path)),
                path_to_string(&dep),
            ]
            .contains(&cmd)
            {
                return format!("$(location :{dep_target_name})");
            }
        }
        for output in outputs {
            if vec![
                path_to_string(canonicalize_path(output, self.build_path)),
                path_to_string(output),
                file_name(output),
            ]
            .contains(&cmd)
            {
                return format!(
                    "$(location {0})",
                    path_to_string(self.map_cmd_output(output))
                );
            }
        }
        for input in inputs {
            let canonicalize_input = canonicalize_path(input, self.build_path);
            for from in vec![
                path_to_string(canonicalize_input.parent().unwrap()),
                path_to_string(input.parent().unwrap()),
            ] {
                if let Some(suffix) = cmd.strip_prefix(&from) {
                    return format!(
                        "$$(dirname $(location {0})){1}",
                        path_to_string(strip_prefix(&canonicalize_input, self.src_path,)),
                        suffix,
                    );
                }
            }
        }
        for output in outputs {
            let output_string = path_to_string(output.parent().unwrap());
            let canonicalize_output = canonicalize_path(&output_string, self.build_path);
            let stripped_output = strip_prefix(&canonicalize_output, self.build_path.join("n2s"));
            let mut froms = vec![
                path_to_string(&canonicalize_output),
                path_to_string(canonicalize_path(&stripped_output, self.build_path)),
            ];
            if !output_string.is_empty() {
                froms.push(output_string);
            }
            for from in froms {
                if let Some(suffix) = cmd.strip_prefix(&from) {
                    return format!(
                        "$$(dirname $(location {0})){1}",
                        path_to_string(self.map_cmd_output(output)),
                        suffix,
                    );
                }
            }
            if path_to_string(stripped_output) == cmd {
                return format!(
                    "$$(dirname $(location {0}))",
                    path_to_string(self.map_cmd_output(output))
                );
            }
        }
        cmd
    }
    fn split_cmd(
        &self,
        mut separators: Vec<&str>,
        cmd: &str,
        inputs: &Vec<PathBuf>,
        outputs: &Vec<PathBuf>,
        deps: &Vec<(PathBuf, String)>,
    ) -> String {
        let Some(separator) = separators.pop() else {
            return self.replace_in_splitted_cmd(cmd, inputs, outputs, deps);
        };
        cmd.split(separator)
            .into_iter()
            .map(|split| self.split_cmd(separators.clone(), split, inputs, outputs, deps))
            .collect::<Vec<_>>()
            .join(separator)
    }
    fn get_cmd(
        &self,
        mut cmd: String,
        rule_cmd: NinjaRuleCmd,
        inputs: Vec<PathBuf>,
        outputs: &Vec<PathBuf>,
        deps: Vec<(PathBuf, String)>,
    ) -> String {
        cmd = self.split_cmd(vec![" ", "=", "-I", ","], &cmd, &inputs, outputs, &deps);
        if let Some((rsp_file, rsp_content)) = rule_cmd.rsp_info {
            let rsp_inputs = rsp_content
                .split(" ")
                .filter_map(|file| {
                    if file.is_empty() {
                        return None;
                    }
                    Some(PathBuf::from(file))
                })
                .collect::<Vec<PathBuf>>();
            let rsp_inputs_string = if inputs == rsp_inputs {
                String::from("$(in)")
            } else {
                rsp_inputs
                    .iter()
                    .map(|file| {
                        let file_path = path_to_string(strip_prefix(
                            canonicalize_path(file, self.build_path),
                            self.src_path,
                        ));
                        format!("$(location {file_path})")
                    })
                    .collect::<Vec<String>>()
                    .join(" ")
            };
            let rsp = format!("$(genDir)/{rsp_file}");
            cmd = format!("echo \\\"{rsp_inputs_string}\\\" > {rsp} && {cmd}")
                .replace("${rspfile}", &rsp);
        }
        cmd
    }
    fn get_dep_id(&self, input: &Path) -> String {
        let Some(target) = self.targets_map.get(&input) else {
            return path_to_id(Path::new(self.project.get_name()).join(&input));
        };
        return if target.get_outputs().len() > 1 {
            path_to_id(Path::new(self.project.get_name()).join(target.get_name()))
                + "{"
                + &path_to_string(&input)
                + "}"
        } else {
            path_to_id(Path::new(self.project.get_name()).join(target.get_name()))
        };
    }
    fn get_cmd_inputs(
        &self,
        inputs: Vec<PathBuf>,
        deps: &mut Vec<(PathBuf, String)>,
    ) -> Vec<PathBuf> {
        inputs
            .into_iter()
            .filter_map(|input| {
                if let Some(mapped_input) = self.project.map_cmd_input(&input) {
                    deps.push((input, mapped_input));
                    return None;
                }
                if canonicalize_path(&input, self.build_path).starts_with(self.build_path) {
                    deps.push((input.clone(), self.get_dep_id(&input)));
                    return None;
                }
                Some(input)
            })
            .collect()
    }
    fn get_tool_module(
        &mut self,
        tool: &Path,
        python_inputs: Vec<PathBuf>,
    ) -> Result<Option<(String, Option<Vec<SoongModule>>)>, String> {
        if let Some(tool_module) = self.project.map_tool_module(tool) {
            self.internals.tools_module.push(tool_module.clone());
            return Ok(Some((path_to_id(tool_module), None)));
        } else if let Ok(tool_target_path) = Path::new(tool).strip_prefix(self.build_path) {
            return Ok(Some((
                String::from(":") + &self.get_dep_id(tool_target_path),
                None,
            )));
        } else if !file_name(tool).ends_with(".py") {
            return Ok(None);
        }
        let tool_module = path_to_id(Path::new(self.project.get_name()).join(tool));
        if !self.internals.python_binaries.contains(&tool_module) {
            let mut modules = Vec::new();
            let (src, main) = if file_stem(tool).contains(".") {
                let new_tool = path_to_id(PathBuf::from(tool)) + ".py";
                let name = path_to_id(Path::new(self.project.get_name()).join(&new_tool));
                modules.push(
                    SoongModule::new("genrule")
                        .add_prop("name", SoongProp::Str(name.clone()))
                        .add_prop("cmd", SoongProp::Str(String::from("cp $(in) $(out)")))
                        .add_prop("srcs", SoongProp::VecStr(vec![path_to_string(tool)]))
                        .add_prop("out", SoongProp::VecStr(vec![new_tool.clone()])),
                );
                (String::from(":") + &name, new_tool)
            } else {
                (path_to_string(tool), path_to_string(tool))
            };

            let mut srcs = Vec::new();
            for python_input in python_inputs {
                let name = path_to_id(
                    Path::new(self.project.get_name())
                        .join(&python_input)
                        .join("cp"),
                );
                let lib_full_name = path_to_string(&python_input);
                if !self.internals.python_libraries.contains(&lib_full_name) {
                    modules.push(
                        SoongModule::new("genrule")
                            .add_prop("name", SoongProp::Str(name.clone()))
                            .add_prop("cmd", SoongProp::Str(String::from("cp $(in) $(out)")))
                            .add_prop("srcs", SoongProp::VecStr(vec![lib_full_name.clone()]))
                            .add_prop("out", SoongProp::VecStr(vec![file_name(&python_input)])),
                    );
                    self.internals.python_libraries.insert(lib_full_name);
                }
                srcs.push(String::from(":") + &name);
            }

            srcs.push(src);
            srcs.sort_unstable();
            srcs.dedup();
            let multiple_srcs = srcs.len() > 1;
            let module = SoongModule::new("python_binary_host")
                .add_prop("name", SoongProp::Str(tool_module.clone()))
                .add_prop("main", SoongProp::Str(main))
                .add_prop("srcs", SoongProp::VecStr(srcs));
            let extended_module = self
                .project
                .extend_python_binary_host(&self.src_path.join(&tool), module.clone())?;
            if !multiple_srcs && module == extended_module {
                return Ok(None);
            }
            self.internals.python_binaries.insert(tool_module.clone());
            modules.push(extended_module);
            return Ok(Some((tool_module, Some(modules))));
        }
        Ok(Some((tool_module, None)))
    }
    fn get_tools(
        &mut self,
        mut cmd: String,
        inputs: &mut Vec<PathBuf>,
    ) -> Result<(Vec<String>, Vec<String>, Vec<SoongModule>, String), String> {
        if cmd.starts_with("cp") {
            return Ok((Vec::new(), Vec::new(), Vec::new(), cmd));
        }
        while let Some(index) = cmd.find("python") {
            let begin = str::from_utf8(&cmd.as_bytes()[0..index])
                .unwrap()
                .rfind(" ")
                .unwrap_or_default();
            cmd = match str::from_utf8(&cmd.as_bytes()[index..]).unwrap().find(" ") {
                Some(end) => cmd.replace(
                    str::from_utf8(&cmd.as_bytes()[begin..index + end + 1]).unwrap(),
                    "",
                ),
                None => cmd.replace(str::from_utf8(&cmd.as_bytes()[begin..]).unwrap(), ""),
            };
        }
        let tool_location = String::from("$(location) ");
        let (tool, mut cmd) = if let Some((tool, cmd)) = cmd.split_once(" ") {
            (String::from(tool), tool_location + cmd)
        } else {
            (String::from(cmd), tool_location)
        };
        let mut tool_modules = Vec::new();
        let glslang_validator = String::from("glslangValidator");
        let tool_location = String::from("$(location ") + &tool + ")";
        for idx in 0..inputs.len() {
            if path_to_string(&inputs[idx]) == tool {
                inputs.remove(idx);
                break;
            }
        }
        for idx in 0..inputs.len() {
            if inputs[idx].ends_with(&glslang_validator) {
                let input = path_to_string(&inputs[idx]);
                inputs.remove(idx);
                tool_modules.push(glslang_validator.clone());
                cmd = cmd.replace("$(location)", &tool_location).replace(
                    &input,
                    &(String::from("$(location ") + &glslang_validator + ")"),
                );
                break;
            }
        }
        *inputs = inputs
            .into_iter()
            .filter(|input| !file_name(input).starts_with("python"))
            .map(|input| input.clone())
            .collect();
        if tool.ends_with(".py") {
            cmd = String::from("python3 ") + &cmd;
        }
        let tool = strip_prefix(canonicalize_path(&tool, self.build_path), self.src_path);
        let python_inputs = inputs
            .iter()
            .filter_map(|input| {
                if path_to_string(input).ends_with(".py") {
                    Some(strip_prefix(
                        canonicalize_path(input, self.build_path),
                        self.src_path,
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if let Some((tool_module, some_modules)) = self.get_tool_module(&tool, python_inputs)? {
            cmd = cmd.replace(
                &tool_location,
                &(String::from("$(location ") + &tool_module + ")"),
            );
            tool_modules.push(tool_module);
            return if let Some(modules) = some_modules {
                Ok((Vec::new(), tool_modules, modules, cmd))
            } else {
                Ok((Vec::new(), tool_modules, Vec::new(), cmd))
            };
        }
        let tool_name = file_name(&tool);
        if ["bison", "flex"].contains(&tool_name.as_str()) {
            tool_modules.push(tool_name.clone());
            tool_modules.push(String::from("m4"));
            return Ok((
                Vec::new(),
                tool_modules,
                Vec::new(),
                String::from("M4=$(location m4) ")
                    + &cmd.replace(
                        "$(location)",
                        &(String::from("$(location ") + &tool_name + ")"),
                    ),
            ));
        }
        if tool_name == "glslangValidator" {
            tool_modules.push(glslang_validator.clone());
            return Ok((Vec::new(), tool_modules, Vec::new(), cmd));
        }
        Ok((vec![path_to_string(tool)], tool_modules, Vec::new(), cmd))
    }
    pub fn generate_custom_command(
        &mut self,
        target: &T,
        rule_cmd: NinjaRuleCmd,
    ) -> Result<Vec<SoongModule>, String> {
        let mut inputs = Vec::new();
        let mut deps = Vec::new();
        inputs.extend(self.get_cmd_inputs(target.get_inputs().clone(), &mut deps));
        inputs.extend(self.get_cmd_inputs(target.get_implicit_deps().clone(), &mut deps));
        let (tool_files, tool_modules, mut modules, cmd) =
            self.get_tools(rule_cmd.command.clone(), &mut inputs)?;
        let mut sources = inputs
            .iter()
            .map(|input| {
                path_to_string(strip_prefix(
                    canonicalize_path(input, self.build_path),
                    self.src_path,
                ))
            })
            .collect::<Vec<String>>();
        for (dep, dep_target_name) in &deps {
            let src = format!(":{dep_target_name}");
            if !tool_modules.contains(&src) {
                sources.push(src);
            }
            if let Some(dep_target) = self.targets_map.get(dep) {
                if !self.filter_target(dep_target) {
                    self.internals.custom_cmd_inputs.push(dep.clone());
                }
            } else {
                self.internals.custom_cmd_inputs.push(dep.clone());
            }
        }
        let target_outputs = target.get_outputs();
        let cmd = self.get_cmd(cmd, rule_cmd, inputs, target_outputs, deps);
        let outputs = target_outputs
            .iter()
            .map(|output| path_to_string(self.map_cmd_output(output)))
            .collect();
        let target_name = target.get_name();
        let module_name = match self.targets_to_gen.get_name(&target_name) {
            Some(name) => path_to_string(name),
            None => path_to_id(Path::new(self.project.get_name()).join(target_name)),
        };

        modules.push(
            self.project.extend_custom_command(
                &target.get_name(),
                SoongModule::new("cc_genrule")
                    .add_prop("name", SoongProp::Str(module_name))
                    .add_prop("cmd", SoongProp::Str(cmd))
                    .add_prop("srcs", SoongProp::VecStr(sources))
                    .add_prop("out", SoongProp::VecStr(outputs))
                    .add_prop("tools", SoongProp::VecStr(tool_modules))
                    .add_prop("tool_files", SoongProp::VecStr(tool_files)),
            )?,
        );
        Ok(modules)
    }
}
