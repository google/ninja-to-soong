// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::utils::*;

mod cc;
mod command;
mod rust;

pub use cc::LibraryKind;

use cc::{
    has_compile_only_flag, has_shared_flag, is_opencl_c_compile, parse_archive_libs,
    parse_cc_compile_args, parse_cc_link_args, strip_depfile_flags, CcCompileInfo, CcLinkInfo,
};
use command::{
    expand_rspfile_tokens, extract_command, is_archiver, is_bindgen, is_c_cpp_compiler, is_rustc,
    split_shell_tokens, ExtractedCommand,
};
use rust::{parse_bindgen_args, parse_rustc_args, BindgenInfo, RustInfo, RustRuleKind};

#[derive(Debug, Clone)]
pub struct NinjaRuleCmd {
    pub command: String,
    pub rsp_info: Option<(String, String)>,
    pub is_soong_cmd: bool,
}

impl NinjaRuleCmd {
    fn new(command: String, rsp_info: Option<(String, String)>) -> Self {
        Self {
            command,
            rsp_info,
            is_soong_cmd: false,
        }
    }

    fn soong_copy() -> Self {
        Self {
            command: String::from("cp $(in) $(out)"),
            rsp_info: None,
            is_soong_cmd: true,
        }
    }
}

pub enum NinjaRule {
    Binary,
    StaticLibrary,
    SharedLibrary,
    RustStaticLibrary,
    RustLibrary,
    RustLibraryHost,
    RustProcMacro,
    RustBindgen,
    CustomCommand(NinjaRuleCmd),
    None,
}

#[derive(Debug)]
pub struct NinjaTargetCommon {
    pub rule: String,
    pub outputs: Vec<PathBuf>,
    pub implicit_outputs: Vec<PathBuf>,
    pub inputs: Vec<PathBuf>,
    pub implicit_deps: Vec<PathBuf>,
    pub order_only_deps: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct ResolvedRuleCommand {
    pub command: String,
    pub rsp_info: Option<(String, String)>,
    pub description: Option<String>,
    pub is_generator: bool,
}

#[derive(Debug)]
enum TargetAction {
    CompileObject(CcCompileInfo),
    StaticLibrary {
        link_info: CcLinkInfo,
        archive_cmd: NinjaRuleCmd,
    },
    SharedLibrary {
        compile_info: CcCompileInfo,
        link_info: CcLinkInfo,
    },
    Binary {
        compile_info: CcCompileInfo,
        link_info: CcLinkInfo,
    },
    Rust(Result<RustInfo, String>),
    RustBindgen(BindgenInfo),
    CustomCommand(NinjaRuleCmd),
    None,
}

#[derive(Debug)]
pub struct NinjaTarget {
    common: NinjaTargetCommon,
    action: TargetAction,
}

fn classify_c_cpp_action(
    primary_cmd: &str,
    args: &[String],
    rsp_info: Option<(String, String)>,
    outputs: &[PathBuf],
    implicit_outputs: &[PathBuf],
) -> TargetAction {
    if is_opencl_c_compile(args) {
        return TargetAction::CustomCommand(NinjaRuleCmd::new(
            strip_depfile_flags(primary_cmd),
            rsp_info,
        ));
    }
    if has_compile_only_flag(args) {
        return TargetAction::CompileObject(parse_cc_compile_args(args));
    }
    let (compile_info, link_info) = parse_cc_link_args(args, outputs, implicit_outputs);
    if has_shared_flag(args) {
        TargetAction::SharedLibrary {
            compile_info,
            link_info,
        }
    } else {
        TargetAction::Binary {
            compile_info,
            link_info,
        }
    }
}

fn classify_build_tool(
    cmd: String,
    rsp_info: Option<(String, String)>,
    outputs: &[PathBuf],
    implicit_outputs: &[PathBuf],
) -> TargetAction {
    let raw_tokens = split_shell_tokens(&cmd);
    let tokens = expand_rspfile_tokens(raw_tokens, rsp_info.as_ref());
    let Some(first) = tokens.first() else {
        return TargetAction::None;
    };
    let exe = file_name(Path::new(first));
    let args = &tokens[1..];

    if is_c_cpp_compiler(&exe) {
        return classify_c_cpp_action(&cmd, args, rsp_info, outputs, implicit_outputs);
    }
    if is_archiver(&exe) {
        return TargetAction::StaticLibrary {
            link_info: parse_archive_libs(args, outputs, implicit_outputs),
            archive_cmd: NinjaRuleCmd::new(cmd, rsp_info),
        };
    }
    if is_rustc(&exe) {
        return TargetAction::Rust(parse_rustc_args(args));
    }
    if is_bindgen(&exe) {
        return TargetAction::RustBindgen(parse_bindgen_args(args));
    }
    TargetAction::CustomCommand(NinjaRuleCmd::new(cmd, rsp_info))
}

fn classify_command(
    resolved: ResolvedRuleCommand,
    outputs: &[PathBuf],
    implicit_outputs: &[PathBuf],
) -> TargetAction {
    if resolved.is_generator {
        return TargetAction::None;
    }
    match extract_command(&resolved.command, resolved.description.as_deref()) {
        None => TargetAction::None,
        Some(ExtractedCommand::BuildTool(cmd)) => {
            classify_build_tool(cmd, resolved.rsp_info, outputs, implicit_outputs)
        }
        Some(ExtractedCommand::Custom(cmd)) => {
            TargetAction::CustomCommand(NinjaRuleCmd::new(cmd, resolved.rsp_info))
        }
    }
}

impl NinjaTarget {
    pub fn new(common: NinjaTargetCommon, resolved: Option<ResolvedRuleCommand>) -> Self {
        let action = match resolved {
            Some(cmd) => classify_command(cmd, &common.outputs, &common.implicit_outputs),
            None => TargetAction::None,
        };
        Self { common, action }
    }

    pub fn is_custom_command(&self) -> bool {
        matches!(self.action, TargetAction::CustomCommand(_))
    }

    pub fn convert_archive_to_custom_command_if_needed(
        &mut self,
        custom_cmd_outputs: &HashSet<PathBuf>,
    ) {
        let should_convert = matches!(self.action, TargetAction::StaticLibrary { .. })
            && !self.common.inputs.is_empty()
            && self
                .common
                .inputs
                .iter()
                .all(|input| custom_cmd_outputs.contains(input));
        if should_convert {
            let prev = std::mem::replace(&mut self.action, TargetAction::None);
            if let TargetAction::StaticLibrary { archive_cmd, .. } = prev {
                self.action = TargetAction::CustomCommand(archive_cmd);
            }
        }
    }

    pub fn split_multi_output(mut self) -> Vec<Self> {
        if self.common.outputs.len() <= 1 {
            return vec![self];
        }
        let TargetAction::CustomCommand(ref mut rule_cmd) = self.action else {
            return vec![self];
        };
        let mut targets = Vec::new();
        for output in self.common.outputs.iter_mut() {
            let renamed = Path::new("n2s").join(&*output);
            rule_cmd.command = rule_cmd
                .command
                .replace(&path_to_string(&*output), &path_to_string(&renamed));
            let old_output = std::mem::replace(output, renamed.clone());
            targets.push(Self {
                common: NinjaTargetCommon {
                    rule: self.common.rule.clone(),
                    outputs: vec![old_output],
                    implicit_outputs: Vec::new(),
                    inputs: vec![renamed],
                    implicit_deps: Vec::new(),
                    order_only_deps: Vec::new(),
                },
                action: TargetAction::CustomCommand(NinjaRuleCmd::soong_copy()),
            });
        }
        targets.push(self);
        targets
    }

    pub fn get_name(&self) -> PathBuf {
        PathBuf::from(&self.common.outputs[0])
    }

    pub fn get_inputs(&self) -> &Vec<PathBuf> {
        &self.common.inputs
    }

    pub fn get_implicit_deps(&self) -> &Vec<PathBuf> {
        &self.common.implicit_deps
    }

    pub fn get_order_only_deps(&self) -> &Vec<PathBuf> {
        &self.common.order_only_deps
    }

    pub fn get_outputs(&self) -> &Vec<PathBuf> {
        &self.common.outputs
    }

    pub fn get_implicit_outputs(&self) -> &Vec<PathBuf> {
        &self.common.implicit_outputs
    }

    pub fn get_rule(&self) -> Result<NinjaRule, String> {
        Ok(match &self.action {
            TargetAction::Binary { .. } => NinjaRule::Binary,
            TargetAction::StaticLibrary { .. } => NinjaRule::StaticLibrary,
            TargetAction::SharedLibrary { .. } => NinjaRule::SharedLibrary,
            TargetAction::Rust(res) => match res.as_ref().map_err(Clone::clone)?.kind {
                RustRuleKind::StaticLibrary => NinjaRule::RustStaticLibrary,
                RustRuleKind::Library => NinjaRule::RustLibrary,
                RustRuleKind::LibraryHost => NinjaRule::RustLibraryHost,
                RustRuleKind::ProcMacro => NinjaRule::RustProcMacro,
            },
            TargetAction::RustBindgen(_) => NinjaRule::RustBindgen,
            TargetAction::CustomCommand(cmd) => NinjaRule::CustomCommand(cmd.clone()),
            TargetAction::CompileObject(_) | TargetAction::None => NinjaRule::None,
        })
    }

    pub fn get_sources(&self, build_path: &Path) -> Result<Vec<PathBuf>, String> {
        match &self.action {
            TargetAction::CompileObject(_) | TargetAction::RustBindgen(_) | TargetAction::None => {
                Ok(self
                    .common
                    .inputs
                    .iter()
                    .map(|input| canonicalize_path(input, build_path))
                    .collect())
            }
            _ => Ok(Vec::new()),
        }
    }

    fn get_link_info(&self) -> Option<&CcLinkInfo> {
        match &self.action {
            TargetAction::StaticLibrary { link_info, .. }
            | TargetAction::SharedLibrary { link_info, .. }
            | TargetAction::Binary { link_info, .. } => Some(link_info),
            _ => None,
        }
    }

    fn get_compile_info(&self) -> Option<&CcCompileInfo> {
        match &self.action {
            TargetAction::CompileObject(info)
            | TargetAction::SharedLibrary {
                compile_info: info, ..
            }
            | TargetAction::Binary {
                compile_info: info, ..
            } => Some(info),
            TargetAction::RustBindgen(info) => Some(&info.cc_info),
            _ => None,
        }
    }

    pub fn get_libs_static(&self) -> Vec<PathBuf> {
        self.get_link_info()
            .map(|info| info.libs_static.clone())
            .unwrap_or_default()
    }

    pub fn get_libs_static_whole(&self) -> Vec<PathBuf> {
        self.get_link_info()
            .map(|info| info.libs_static_whole.clone())
            .unwrap_or_default()
    }

    pub fn get_libs_shared(&self) -> Vec<PathBuf> {
        self.get_link_info()
            .map(|info| info.libs_shared.clone())
            .unwrap_or_default()
    }

    pub fn get_link_flags(&self) -> (Option<PathBuf>, Vec<String>) {
        self.get_link_info()
            .map(|info| (info.version_script.clone(), info.link_flags.clone()))
            .unwrap_or_else(|| (None, Vec::new()))
    }

    pub fn get_defines(&self) -> Vec<String> {
        self.get_compile_info()
            .map(|info| info.defines.clone())
            .unwrap_or_default()
    }

    pub fn get_includes(&self, build_path: &Path) -> Vec<PathBuf> {
        self.get_compile_info()
            .map(|info| {
                info.includes
                    .iter()
                    .map(|dir| canonicalize_path(dir, build_path))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_cflags(&self) -> Vec<String> {
        self.get_compile_info()
            .map(|info| info.cflags.clone())
            .unwrap_or_default()
    }

    pub fn get_c_std(&self) -> Option<String> {
        self.get_compile_info().and_then(|info| info.c_std.clone())
    }

    pub fn get_bindgen_flags(&self) -> Vec<String> {
        match &self.action {
            TargetAction::RustBindgen(info) => info.flags.clone(),
            _ => Vec::new(),
        }
    }

    pub fn get_crate_name(&self) -> Result<String, String> {
        match &self.action {
            TargetAction::Rust(res) => Ok(res.as_ref().map_err(Clone::clone)?.crate_name.clone()),
            _ => error!("get_crate_name called on non-Rust target: {self:#?}"),
        }
    }

    pub fn get_edition(&self) -> Result<String, String> {
        match &self.action {
            TargetAction::Rust(res) => Ok(res.as_ref().map_err(Clone::clone)?.edition.clone()),
            _ => error!("get_edition called on non-Rust target: {self:#?}"),
        }
    }

    pub fn get_cfgs(&self) -> Vec<String> {
        match &self.action {
            TargetAction::Rust(Ok(info)) => info.cfgs.clone(),
            _ => Vec::new(),
        }
    }

    pub fn get_rustlibs(&self) -> Vec<PathBuf> {
        match &self.action {
            TargetAction::Rust(Ok(info)) => info.rustlibs.clone(),
            _ => Vec::new(),
        }
    }

    pub fn get_proc_macros(&self) -> Vec<PathBuf> {
        match &self.action {
            TargetAction::Rust(Ok(info)) => info.proc_macros.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NinjaTargetToGenMapEntry {
    pub name: Option<PathBuf>,
    pub stem: Option<String>,
    pub module_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NinjaTargetToGen {
    pub path: String,
    pub entry: NinjaTargetToGenMapEntry,
}

pub struct NinjaTargetsToGenMap(HashMap<PathBuf, NinjaTargetToGenMapEntry>);
impl NinjaTargetsToGenMap {
    pub fn get_name(&self, target: &Path) -> Option<PathBuf> {
        self.0.get(target)?.name.clone()
    }
    pub fn get_stem(&self, target: &Path) -> Option<String> {
        self.0.get(target)?.stem.clone()
    }
    pub fn get_module_name(&self, target: &Path) -> Option<String> {
        self.0.get(target)?.module_type.clone()
    }
    pub fn get_targets(&self) -> Vec<PathBuf> {
        let mut vec: Vec<PathBuf> = self.0.keys().cloned().collect();
        vec.sort_unstable();
        vec
    }
    fn insert(&mut self, target: &NinjaTargetToGen) {
        self.0
            .insert(PathBuf::from(&target.path), target.entry.clone());
    }
    pub fn from(targets: &[NinjaTargetToGen]) -> Self {
        let mut map = Self(HashMap::new());
        for target in targets {
            map.insert(target);
        }
        map
    }
    pub fn push(mut self, target: NinjaTargetToGen) -> Self {
        self.insert(&target);
        self
    }
}

#[derive(Debug)]
pub struct NinjaTargetsMap<'a>(HashMap<&'a Path, &'a NinjaTarget>);

impl<'a> NinjaTargetsMap<'a> {
    pub fn new(targets: &'a [NinjaTarget]) -> Self {
        let mut map = HashMap::new();
        for target in targets {
            for output in target.get_outputs() {
                map.insert(output.as_path(), target);
            }
            for output in target.get_implicit_outputs() {
                map.insert(output.as_path(), target);
            }
        }
        Self(map)
    }
    pub fn get(&self, key: &Path) -> Option<&&NinjaTarget> {
        self.0.get(key)
    }
    pub fn traverse_from<F>(
        &self,
        mut targets: Vec<PathBuf>,
        only_inputs: bool,
        mut filter_target: F,
    ) -> Result<(), String>
    where
        F: FnMut(&NinjaTarget) -> Result<bool, String>,
    {
        let mut targets_seen = HashSet::new();
        while let Some(target_name) = targets.pop() {
            if targets_seen.contains(&target_name) {
                continue;
            }
            targets_seen.insert(target_name.clone());
            let Some(target) = self.get(&target_name) else {
                continue;
            };
            targets_seen.extend(target.get_outputs().clone());
            targets_seen.extend(target.get_implicit_outputs().clone());
            if filter_target(target)? {
                targets.extend(target.get_inputs().clone());
                if !only_inputs {
                    targets.extend(target.get_implicit_deps().clone());
                    targets.extend(target.get_order_only_deps().clone());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_common(outputs: &[&str], inputs: &[&str]) -> NinjaTargetCommon {
        NinjaTargetCommon {
            rule: String::from("test_rule"),
            outputs: outputs.iter().map(PathBuf::from).collect(),
            implicit_outputs: Vec::new(),
            inputs: inputs.iter().map(PathBuf::from).collect(),
            implicit_deps: Vec::new(),
            order_only_deps: Vec::new(),
        }
    }

    #[test]
    fn unsupported_rustc_target_fails_lazily_on_access() {
        let target = NinjaTarget::new(
            make_common(&["helper_bin"], &["src/main.rs"]),
            Some(ResolvedRuleCommand {
                command: String::from("rustc --crate-type=bin --crate-name=helper src/main.rs"),
                rsp_info: None,
                description: None,
                is_generator: false,
            }),
        );
        assert!(target.get_rule().is_err());
        assert!(target.get_crate_name().is_err());
        assert!(target.get_edition().is_err());
    }

    #[test]
    fn opencl_c_compile_preserves_rsp_info() {
        let rsp = (String::from("k.rsp"), String::from("-DCL=1"));
        let target = NinjaTarget::new(
            make_common(&["k.bc"], &["k.cl"]),
            Some(ResolvedRuleCommand {
                command: String::from("clang -x cl -MD -MF k.d @${rspfile} -c -o k.bc k.cl"),
                rsp_info: Some(rsp.clone()),
                description: None,
                is_generator: false,
            }),
        );
        match target.get_rule().unwrap() {
            NinjaRule::CustomCommand(cmd) => {
                assert_eq!(cmd.command, "clang -x cl @${rspfile} -c -o k.bc k.cl");
                assert_eq!(cmd.rsp_info, Some(rsp));
            }
            _ => panic!("expected CustomCommand for OpenCL C compile"),
        }
    }

    #[test]
    fn archive_conversion_requires_all_inputs_to_be_custom_commands() {
        let custom_outputs: HashSet<PathBuf> = [PathBuf::from("k.bc")].into_iter().collect();

        let mut all_custom = NinjaTarget::new(
            make_common(&["kernel.a"], &["k.bc"]),
            Some(ResolvedRuleCommand {
                command: String::from("ar qc kernel.a k.bc"),
                rsp_info: None,
                description: None,
                is_generator: false,
            }),
        );
        all_custom.convert_archive_to_custom_command_if_needed(&custom_outputs);
        assert!(all_custom.is_custom_command());

        let mut mixed = NinjaTarget::new(
            make_common(&["mixed.a"], &["k.bc", "normal.o"]),
            Some(ResolvedRuleCommand {
                command: String::from("ar rcs mixed.a k.bc normal.o"),
                rsp_info: None,
                description: None,
                is_generator: false,
            }),
        );
        mixed.convert_archive_to_custom_command_if_needed(&custom_outputs);
        assert!(matches!(
            mixed.get_rule().unwrap(),
            NinjaRule::StaticLibrary
        ));
    }

    #[test]
    fn split_multi_output_marks_copy_wrappers_as_soong_cmd() {
        let target = NinjaTarget::new(
            make_common(&["gen.c", "gen.h"], &["spec.in"]),
            Some(ResolvedRuleCommand {
                command: String::from("gen.sh spec.in gen.c gen.h"),
                rsp_info: None,
                description: None,
                is_generator: false,
            }),
        );
        let split = target.split_multi_output();
        assert_eq!(split.len(), 3);
        for wrapper in &split[..2] {
            match wrapper.get_rule().unwrap() {
                NinjaRule::CustomCommand(cmd) => {
                    assert!(cmd.is_soong_cmd);
                    assert_eq!(cmd.command, "cp $(in) $(out)");
                }
                _ => panic!("expected CustomCommand wrapper"),
            }
        }
        match split[2].get_rule().unwrap() {
            NinjaRule::CustomCommand(cmd) => {
                assert!(!cmd.is_soong_cmd);
                assert_eq!(cmd.command, "gen.sh spec.in n2s/gen.c n2s/gen.h");
            }
            _ => panic!("expected main CustomCommand target"),
        }
    }
}
