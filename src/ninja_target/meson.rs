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
        if !split.0.contains("meson") {
            return common::get_cmd(command);
        }
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
    fn get_c_args(&self) -> Option<&str> {
        if let Some(args) = self.0.variables.get("ARGS") {
            return Some(args);
        }
        self.0
            .variables
            .get("COMMAND")?
            .split_once(" -- ")
            .map(|(_, args)| args)
    }
    fn get_rust_args(&self, flag: &str) -> Vec<String> {
        let Some(args) = self.0.variables.get("ARGS") else {
            return Vec::new();
        };
        let flag_eq = format!("{flag}=");
        let mut values = Vec::new();
        let mut iter = args.split(" ");
        while let Some(arg) = iter.next() {
            if arg == flag {
                if let Some(val) = iter.next() {
                    values.push(String::from(val.trim_matches('\'')));
                }
            } else if let Some(val) = arg.strip_prefix(&flag_eq) {
                values.push(String::from(val.trim_matches('\'')));
            }
        }
        values
    }
    fn get_rust_arg(&self, flag: &str) -> Result<String, String> {
        let Some(arg) = self.get_rust_args(flag).pop() else {
            return error!("No {flag} in: {self:#?}");
        };
        Ok(arg)
    }
    fn get_rust_externs(&self, ext: &str) -> Vec<PathBuf> {
        self.get_rust_args("--extern")
            .into_iter()
            .filter_map(|arg| {
                let (_, path) = arg.split_once("=")?;
                path.ends_with(ext).then(|| PathBuf::from(path))
            })
            .collect()
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
            } else if self.0.rule == "rust_COMPILER" {
                match self.get_rust_arg("--crate-type")?.as_str() {
                    "staticlib" => NinjaRule::RustStaticLibrary,
                    "rlib" => NinjaRule::RustLibrary,
                    crate_type => {
                        return error!("Unsupported crate_type ({crate_type}) in: {self:#?}")
                    }
                }
            } else if self.0.rule == "rust_COMPILER_FOR_BUILD" {
                match self.get_rust_arg("--crate-type")?.as_str() {
                    "rlib" => NinjaRule::RustLibraryHost,
                    "proc-macro" => NinjaRule::RustProcMacro,
                    crate_type => {
                        return error!("Unsupported crate_type ({crate_type}) in: {self:#?}")
                    }
                }
            } else if self.0.rule == "CUSTOM_COMMAND" || self.0.rule == "CUSTOM_COMMAND_DEP" {
                let Some(command) = self.0.variables.get("COMMAND") else {
                    return error!("No command in: {self:#?}");
                };
                if command.contains("bindgen ") {
                    NinjaRule::RustBindgen
                } else {
                    NinjaRule::CustomCommand(NinjaRuleCmd {
                        command: self.get_command(command),
                        rsp_info: None,
                    })
                }
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
        let Some(args) = self.get_c_args() else {
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
        let Some(args) = self.get_c_args() else {
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
        let Some(args) = self.get_c_args() else {
            return Vec::new();
        };
        let cflags = args
            .split(" ")
            .map(|arg| arg.trim_matches('\''))
            .filter(|arg| !arg.starts_with("-I") && !arg.starts_with("-D"))
            .collect::<Vec<&str>>();
        common::get_cflags(&cflags.join(" "))
    }
    fn get_c_std(&self) -> Option<String> {
        self.get_c_args()?
            .split(" ")
            .map(|arg| arg.trim_matches('\''))
            .find_map(|arg| arg.strip_prefix("-std=").map(String::from))
    }
    fn get_bindgen_flags(&self) -> Vec<String> {
        let Some((bindgen_args, _)) = self
            .0
            .variables
            .get("COMMAND")
            .and_then(|cmd| cmd.split_once(" -- "))
        else {
            return Vec::new();
        };
        let mut args = Vec::new();
        let mut iter = bindgen_args.split(" ").skip(2);
        while let Some(arg) = iter.next() {
            let mut token = String::from(arg);
            while token.ends_with('$') {
                token.pop();
                if let Some(next) = iter.next() {
                    token.push(' ');
                    token.push_str(next);
                }
            }
            args.push(token);
        }
        let mut flags = Vec::new();
        let mut iter = args.into_iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "--experimental" || arg == "--wrap-static-fns" {
                continue;
            }
            if iter.peek().is_some_and(|next| !next.starts_with("--")) {
                let next = iter.next().unwrap();
                let val = next.trim_matches('\'');
                if arg == "--output"
                    || arg == "--depfile"
                    || arg == "--wrap-static-fns-path"
                    || arg == "--rust-target"
                {
                    continue;
                }
                flags.push(format!("{arg}={val}"));
            } else {
                flags.push(arg);
            }
        }
        flags
    }
    fn get_crate_name(&self) -> Result<String, String> {
        self.get_rust_arg("--crate-name")
    }
    fn get_edition(&self) -> Result<String, String> {
        self.get_rust_arg("--edition")
    }
    fn get_cfgs(&self) -> Vec<String> {
        self.get_rust_args("--cfg")
            .into_iter()
            .map(|cfg| cfg.replace("\"", "\\\""))
            .collect()
    }
    fn get_rustlibs(&self) -> Vec<PathBuf> {
        self.get_rust_externs(".rlib")
    }
    fn get_proc_macros(&self) -> Vec<PathBuf> {
        self.get_rust_externs(".so")
    }
}
