// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::*;

use super::cc::{escape_soong_quotes, parse_cc_compile_args, CcCompileInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustRuleKind {
    StaticLibrary,
    Library,
    LibraryHost,
    ProcMacro,
}

#[derive(Debug)]
pub struct RustInfo {
    pub kind: RustRuleKind,
    pub crate_name: String,
    pub edition: String,
    pub cfgs: Vec<String>,
    pub rustlibs: Vec<PathBuf>,
    pub proc_macros: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct BindgenInfo {
    pub flags: Vec<String>,
    pub cc_info: CcCompileInfo,
}

fn collect_flag_values(tokens: &[String], flag: &str) -> Vec<String> {
    let flag_eq = format!("{flag}=");
    let mut values = Vec::new();
    let mut iter = tokens.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            if let Some(val) = iter.next() {
                values.push(val.clone());
            }
        } else if let Some(val) = arg.strip_prefix(&flag_eq) {
            values.push(String::from(val));
        }
    }
    values
}

fn find_last_flag_value(tokens: &[String], flag: &str) -> Option<String> {
    collect_flag_values(tokens, flag).pop()
}

fn classify_rust_kind(tokens: &[String]) -> Result<RustRuleKind, String> {
    let Some(crate_type) = find_last_flag_value(tokens, "--crate-type") else {
        return error!("No --crate-type in rustc args: {tokens:#?}");
    };
    let is_android_target =
        find_last_flag_value(tokens, "--target").is_some_and(|target| target.contains("android"));
    match crate_type.as_str() {
        "staticlib" => Ok(RustRuleKind::StaticLibrary),
        "proc-macro" => Ok(RustRuleKind::ProcMacro),
        "rlib" if is_android_target => Ok(RustRuleKind::Library),
        "rlib" => Ok(RustRuleKind::LibraryHost),
        _ => error!("Unsupported crate_type ({crate_type}) in rustc args: {tokens:#?}"),
    }
}

fn collect_rust_externs(tokens: &[String], ext: &str) -> Vec<PathBuf> {
    collect_flag_values(tokens, "--extern")
        .into_iter()
        .filter_map(|arg| {
            let (_, path) = arg.split_once('=')?;
            path.ends_with(ext).then(|| PathBuf::from(path))
        })
        .collect()
}

pub fn parse_rustc_args(tokens: &[String]) -> Result<RustInfo, String> {
    let kind = classify_rust_kind(tokens)?;
    let Some(crate_name) = find_last_flag_value(tokens, "--crate-name") else {
        return error!("No --crate-name in rustc args: {tokens:#?}");
    };
    let Some(edition) = find_last_flag_value(tokens, "--edition") else {
        return error!("No --edition in rustc args: {tokens:#?}");
    };
    let cfgs = collect_flag_values(tokens, "--cfg")
        .into_iter()
        .map(|cfg| escape_soong_quotes(&cfg))
        .collect();
    let rustlibs = collect_rust_externs(tokens, ".rlib");
    let proc_macros = collect_rust_externs(tokens, ".so");

    Ok(RustInfo {
        kind,
        crate_name,
        edition,
        cfgs,
        rustlibs,
        proc_macros,
    })
}

fn is_skipped_bindgen_flag(flag: &str) -> bool {
    matches!(flag, "--experimental" | "--wrap-static-fns")
}

fn is_skipped_bindgen_value_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--output" | "--depfile" | "--wrap-static-fns-path" | "--rust-target"
    )
}

fn filter_bindgen_flags(tokens: &[String]) -> Vec<String> {
    let mut flags = Vec::new();
    let mut iter = tokens.iter().peekable();

    while let Some(arg) = iter.next() {
        if !arg.starts_with("--") {
            continue;
        }
        if is_skipped_bindgen_flag(arg) {
            continue;
        }
        if let Some((flag_name, _)) = arg.split_once('=') {
            if !is_skipped_bindgen_value_flag(flag_name) {
                flags.push(arg.clone());
            }
            continue;
        }
        if iter.peek().is_some_and(|next| !next.starts_with("--")) {
            let val = iter.next().unwrap();
            if !is_skipped_bindgen_value_flag(arg) {
                flags.push(format!("{arg}={val}"));
            }
        } else {
            flags.push(arg.clone());
        }
    }
    flags
}

pub fn parse_bindgen_args(tokens: &[String]) -> BindgenInfo {
    let (bindgen_args, clang_args) = match tokens.iter().position(|tok| tok == "--") {
        Some(idx) => (&tokens[..idx], &tokens[idx + 1..]),
        None => (tokens, &[][..]),
    };
    BindgenInfo {
        flags: filter_bindgen_flags(bindgen_args),
        cc_info: parse_cc_compile_args(clang_args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn parse_rustc_args_classifies_all_crate_kinds() {
        let static_info = parse_rustc_args(&vec_str(&[
            "--crate-type=staticlib",
            "--crate-name",
            "my_static",
            "--edition=2021",
            "--cfg",
            r#"feature="fast""#,
            "--extern",
            "dep=libdep.rlib",
            "--extern=mac=libmac.so",
        ]))
        .unwrap();
        assert_eq!(static_info.kind, RustRuleKind::StaticLibrary);
        assert_eq!(static_info.crate_name, "my_static");
        assert_eq!(static_info.edition, "2021");
        assert_eq!(static_info.cfgs, vec_str(&[r#"feature=\"fast\""#]));
        assert_eq!(static_info.rustlibs, vec![PathBuf::from("libdep.rlib")]);
        assert_eq!(static_info.proc_macros, vec![PathBuf::from("libmac.so")]);

        let android_rlib = parse_rustc_args(&vec_str(&[
            "--crate-type",
            "rlib",
            "--target",
            "aarch64-linux-android",
            "--crate-name=target_lib",
            "--edition",
            "2021",
        ]))
        .unwrap();
        assert_eq!(android_rlib.kind, RustRuleKind::Library);

        let host_rlib = parse_rustc_args(&vec_str(&[
            "--crate-type=rlib",
            "--crate-name=host_lib",
            "--edition=2021",
        ]))
        .unwrap();
        assert_eq!(host_rlib.kind, RustRuleKind::LibraryHost);

        let proc_macro = parse_rustc_args(&vec_str(&[
            "--crate-type=proc-macro",
            "--crate-name=mac",
            "--edition=2021",
        ]))
        .unwrap();
        assert_eq!(proc_macro.kind, RustRuleKind::ProcMacro);
    }

    #[test]
    fn parse_rustc_args_rejects_missing_or_unsupported_flags() {
        assert!(parse_rustc_args(&vec_str(&["--crate-name=a", "--edition=2021"])).is_err());
        assert!(parse_rustc_args(&vec_str(&[
            "--crate-type=bin",
            "--crate-name=a",
            "--edition=2021"
        ]))
        .is_err());
        assert!(parse_rustc_args(&vec_str(&["--crate-type=rlib", "--edition=2021"])).is_err());
        assert!(parse_rustc_args(&vec_str(&["--crate-type=rlib", "--crate-name=a"])).is_err());
    }

    #[test]
    fn parse_bindgen_args_separates_and_normalizes_flags() {
        let info = parse_bindgen_args(&vec_str(&[
            "wrapper.h",
            "--output",
            "bindings.rs",
            "--depfile=bindings.d",
            "--experimental",
            "--wrap-static-fns",
            "--wrap-static-fns-path",
            "bindings.c",
            "--rust-target=1.78",
            "--no-layout-tests",
            "--allowlist-type",
            "Foo",
            "--blocklist-type=Bar",
            "--",
            "-DFOO=1",
            "-Iinc",
            "-std=c11",
        ]));
        assert_eq!(
            info.flags,
            vec_str(&[
                "--no-layout-tests",
                "--allowlist-type=Foo",
                "--blocklist-type=Bar",
            ])
        );
        assert_eq!(info.cc_info.defines, vec_str(&["FOO=1"]));
        assert_eq!(info.cc_info.includes, vec_str(&["inc"]));
        assert_eq!(info.cc_info.c_std, Some(String::from("c11")));
    }
}
