// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Debug)]
pub struct MesonNinjaTarget(NinjaTargetCommon);

impl MesonNinjaTarget {
    fn get_command(&self, command: &str) -> String {
        let Some(split) = command.split_once(" -- ") else {
            return common::get_cmd(command);
        };
        let Some(capture) = split.0.split_once("--capture ") else {
            return common::get_cmd(split.1);
        };
        return common::get_cmd(split.1)
            + " > "
            + if let Some(output) = capture.1.split_once(" ") {
                output.0
            } else {
                capture.1
            };
    }
}

impl NinjaTarget for MesonNinjaTarget {
    fn new(common: NinjaTargetCommon) -> Self {
        Self(common)
    }
    fn get_common(&self) -> &NinjaTargetCommon {
        &self.0
    }

    fn get_rule(&self) -> Result<NinjaRule, String> {
        Ok(
            if self.0.rule.starts_with("c_LINKER") || self.0.rule.starts_with("cpp_LINKER") {
                let (_, link_flags) = self.get_link_flags();
                if link_flags.contains(&String::from("-fPIC")) {
                    NinjaRule::SharedLibrary
                } else {
                    NinjaRule::Binary
                }
            } else if self.0.rule == "STATIC_LINKER" {
                NinjaRule::StaticLibrary
            } else if self.0.rule == "CUSTOM_COMMAND" || self.0.rule == "CUSTOM_COMMAND_DEP" {
                let Some(command) = self.0.variables.get("COMMAND") else {
                    return error!("No command in: {self:#?}");
                };
                NinjaRule::CustomCommand(NinjaRuleCmd {
                    command: self.get_command(command),
                    rsp_info: None,
                })
            } else {
                NinjaRule::None
            },
        )
    }
    fn get_sources(&self, build_path: &Path) -> Result<Vec<PathBuf>, String> {
        if self.0.inputs.len() != 1 {
            return error!("Too many inputs in: {self:#?}");
        }
        Ok(common::get_sources(&self.0.inputs, build_path))
    }
    fn get_libs_static_whole(&self) -> Vec<PathBuf> {
        let Some(args) = self.0.variables.get("LINK_ARGS") else {
            return Vec::new();
        };
        common::get_libs_static_whole(args)
    }
    fn get_libs_static(&self) -> Vec<PathBuf> {
        let Some(args) = self.0.variables.get("LINK_ARGS") else {
            return Vec::new();
        };
        common::get_libs_static(args)
    }
    fn get_libs_shared(&self) -> Vec<PathBuf> {
        let Some(args) = self.0.variables.get("LINK_ARGS") else {
            return Vec::new();
        };
        common::get_libs_shared(args)
    }
    fn get_link_flags(&self) -> (Option<PathBuf>, Vec<String>) {
        let Some(args) = self.0.variables.get("LINK_ARGS") else {
            return (None, Vec::new());
        };
        let flags = args
            .split(" ")
            .filter(|arg| {
                arg.starts_with("-Wl")
                    || (!arg.starts_with("-l") && !arg.contains(".a") && !arg.contains(".so"))
            })
            .collect::<Vec<&str>>();
        common::get_link_flags(&flags.join(" "))
    }
    fn get_defines(&self) -> Vec<String> {
        let Some(args) = self.0.variables.get("ARGS") else {
            return Vec::new();
        };
        let defines = args
            .split(" ")
            .map(|arg| arg.trim_matches('\''))
            .filter_map(|arg| {
                if !arg.starts_with("-D") {
                    return None;
                }
                Some(arg.replace("\"", "\\\""))
            })
            .collect::<Vec<String>>();
        common::get_defines(&defines.join(" "))
    }
    fn get_includes(&self, build_path: &Path) -> Vec<PathBuf> {
        let Some(args) = self.0.variables.get("ARGS") else {
            return Vec::new();
        };
        let includes = args
            .split(" ")
            .map(|arg| arg.trim_matches('\''))
            .filter(|arg| arg.starts_with("-I"))
            .collect::<Vec<&str>>();
        common::get_includes(&includes.join(" "), build_path)
    }
    fn get_cflags(&self) -> Vec<String> {
        let Some(args) = self.0.variables.get("ARGS") else {
            return Vec::new();
        };
        let cflags = args
            .split(" ")
            .map(|arg| arg.trim_matches('\''))
            .filter(|arg| !arg.starts_with("-I") && !arg.starts_with("-D"))
            .collect::<Vec<&str>>();
        common::get_cflags(&cflags.join(" "))
    }
}
