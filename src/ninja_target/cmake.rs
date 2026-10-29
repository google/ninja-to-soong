// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Debug)]
pub struct CmakeNinjaTarget(NinjaTargetCommon);

impl NinjaTarget for CmakeNinjaTarget {
    fn new(common: NinjaTargetCommon) -> Self {
        Self(common)
    }
    fn get_common(&self) -> &NinjaTargetCommon {
        &self.0
    }

    fn get_rule(&self) -> Result<NinjaRule, String> {
        Ok(
            if self.0.rule.starts_with("CXX_SHARED_LIBRARY")
                || self.0.rule.starts_with("C_SHARED_LIBRARY")
            {
                NinjaRule::SharedLibrary
            } else if self.0.rule.starts_with("CXX_STATIC_LIBRARY")
                || self.0.rule.starts_with("C_STATIC_LIBRARY")
            {
                NinjaRule::StaticLibrary
            } else if self.0.rule.starts_with("CLC_STATIC_LIBRARY") {
                let command = String::from("llvm-ar qc ")
                    + &path_to_string(self.0.outputs[0].clone())
                    + &self.0.inputs.iter().fold(String::new(), |str, input| {
                        str + " " + &path_to_string(input)
                    });
                NinjaRule::CustomCommand(NinjaRuleCmd {
                    command,
                    rsp_info: None,
                })
            } else if self.0.rule.starts_with("CLC_COMPILER") {
                let default = String::new();
                let command = String::from("clang -x cl -c ")
                    + self.0.variables.get("FLAGS").unwrap_or(&default)
                    + " "
                    + self.0.variables.get("DEFINES").unwrap_or(&default)
                    + " "
                    + self.0.variables.get("INCLUDES").unwrap_or(&default)
                    + " "
                    + " -o "
                    + &path_to_string(self.0.outputs[0].clone())
                    + &self.0.inputs.iter().fold(String::new(), |str, input| {
                        str + " " + &path_to_string(input)
                    });
                NinjaRule::CustomCommand(NinjaRuleCmd {
                    command,
                    rsp_info: None,
                })
            } else if self.0.rule.starts_with("CUSTOM_COMMAND") {
                let Some(command) = self.0.variables.get("COMMAND") else {
                    return error!("No command in: {self:#?}");
                };
                let mut split = command.split(" && ");
                let split_count = split.clone().count();
                let command = split.nth(if split_count < 2 { 0 } else { 1 }).unwrap();

                let is_cmake_gen_cmd = (command.contains(" -P ")
                    || command.contains(" -E copy_if_different "))
                    && self
                        .0
                        .variables
                        .get("DESC")
                        .is_some_and(|desc| desc.starts_with("Generating "));
                if command.contains("bin/cmake ") && !is_cmake_gen_cmd {
                    NinjaRule::None
                } else {
                    NinjaRule::CustomCommand(NinjaRuleCmd {
                        command: common::get_cmd(command),
                        rsp_info: None,
                    })
                }
            } else if self.0.rule.starts_with("CXX_EXECUTABLE") {
                NinjaRule::Binary
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
        let Some(libs) = self.0.variables.get("LINK_LIBRARIES") else {
            return Vec::new();
        };
        common::get_libs_static_whole(libs)
    }
    fn get_libs_static(&self) -> Vec<PathBuf> {
        let Some(libs) = self.0.variables.get("LINK_LIBRARIES") else {
            return Vec::new();
        };
        common::get_libs_static(libs)
    }
    fn get_libs_shared(&self) -> Vec<PathBuf> {
        let Some(libs) = self.0.variables.get("LINK_LIBRARIES") else {
            return Vec::new();
        };
        common::get_libs_shared(libs)
    }
    fn get_link_flags(&self) -> (Option<PathBuf>, Vec<String>) {
        let Some(flags) = self.0.variables.get("LINK_FLAGS") else {
            return (None, Vec::new());
        };
        common::get_link_flags(flags)
    }
    fn get_defines(&self) -> Vec<String> {
        let Some(defs) = self.0.variables.get("DEFINES") else {
            return Vec::new();
        };
        common::get_defines(defs)
    }
    fn get_includes(&self, build_path: &Path) -> Vec<PathBuf> {
        let Some(incs) = self.0.variables.get("INCLUDES") else {
            return Vec::new();
        };
        common::get_includes(incs, build_path)
    }
    fn get_cflags(&self) -> Vec<String> {
        let Some(flags) = self.0.variables.get("FLAGS") else {
            return Vec::new();
        };
        common::get_cflags(flags)
    }
}
