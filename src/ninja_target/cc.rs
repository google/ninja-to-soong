// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::iter::Peekable;
use std::slice::Iter;

use crate::utils::*;

use super::command::{split_raw_shell_tokens, unquote_shell_token};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum LibraryKind {
    Shared,
    Static,
    StaticWhole,
    Unspecified,
}

#[derive(Debug, Default)]
pub struct CcCompileInfo {
    pub defines: Vec<String>,
    pub includes: Vec<String>,
    pub cflags: Vec<String>,
    pub c_std: Option<String>,
}

#[derive(Debug, Default)]
pub struct CcLinkInfo {
    pub libs_static: Vec<PathBuf>,
    pub libs_static_whole: Vec<PathBuf>,
    pub libs_shared: Vec<PathBuf>,
    pub version_script: Option<PathBuf>,
    pub link_flags: Vec<String>,
}

pub fn is_opencl_c_compile(tokens: &[String]) -> bool {
    tokens
        .windows(2)
        .any(|pair| pair[0] == "-x" && pair[1] == "cl")
        || tokens.iter().any(|tok| tok == "-xcl")
}

pub fn has_compile_only_flag(tokens: &[String]) -> bool {
    tokens.iter().any(|tok| tok == "-c")
}

pub fn has_shared_flag(tokens: &[String]) -> bool {
    tokens.iter().any(|tok| tok == "-shared")
}

pub fn escape_soong_quotes(value: &str) -> String {
    value.replace('"', "\\\"")
}

fn is_depfile_flag_with_arg(arg: &str) -> bool {
    matches!(arg, "-MT" | "-MQ" | "-MF")
}

fn is_depfile_flag_standalone(arg: &str) -> bool {
    matches!(arg, "-MD" | "-MMD" | "-MP")
        || arg.starts_with("-MT")
        || arg.starts_with("-MQ")
        || arg.starts_with("-MF")
}

pub fn strip_depfile_flags(command: &str) -> String {
    let raw_tokens = split_raw_shell_tokens(command);
    let mut kept = Vec::with_capacity(raw_tokens.len());
    let mut iter = raw_tokens.into_iter();
    while let Some(raw_tok) = iter.next() {
        let tok = unquote_shell_token(raw_tok);
        if is_depfile_flag_with_arg(&tok) {
            iter.next();
            continue;
        }
        if !is_depfile_flag_standalone(&tok) {
            kept.push(raw_tok);
        }
    }
    kept.join(" ")
}

fn try_parse_define_or_include(
    arg: &str,
    iter: &mut Peekable<Iter<String>>,
    info: &mut CcCompileInfo,
) -> bool {
    if arg == "-D" {
        if let Some(val) = iter.next() {
            info.defines.push(escape_soong_quotes(val));
        }
    } else if let Some(val) = arg.strip_prefix("-D") {
        info.defines.push(escape_soong_quotes(val));
    } else if arg == "-I" {
        if let Some(dir) = iter.next() {
            info.includes.push(dir.clone());
        }
    } else if let Some(dir) = arg.strip_prefix("-I") {
        info.includes.push(String::from(dir));
    } else if arg == "-isystem" {
        if let Some(&next) = iter.peek() {
            if !next.starts_with('-') {
                info.includes.push(iter.next().unwrap().clone());
            }
        }
    } else if let Some(dir) = arg.strip_prefix("-isystem") {
        info.includes.push(String::from(dir));
    } else {
        return false;
    }
    true
}

fn is_attached_output_flag(arg: &str) -> bool {
    let Some(val) = arg.strip_prefix("-o") else {
        return false;
    };
    !val.is_empty()
        && !arg.starts_with("-objcmt-")
        && !arg.starts_with("-object_path_lto")
        && !arg.starts_with("-offload-")
}

pub fn parse_cc_compile_args(tokens: &[String]) -> CcCompileInfo {
    let mut info = CcCompileInfo::default();
    let mut iter = tokens.iter().peekable();

    while let Some(arg) = iter.next() {
        if arg == "-o" || is_depfile_flag_with_arg(arg) {
            iter.next();
            continue;
        }
        if arg == "-c" || is_attached_output_flag(arg) || is_depfile_flag_standalone(arg) {
            continue;
        }
        if try_parse_define_or_include(arg, &mut iter, &mut info) {
            continue;
        }
        if arg.starts_with('-') {
            if let Some(std_val) = arg.strip_prefix("-std=") {
                info.c_std = Some(String::from(std_val));
            }
            info.cflags.push(arg.clone());
        }
    }
    info
}

fn is_system_lib(library: &str) -> bool {
    matches!(library, "dl" | "m" | "c" | "pthread" | "atomic" | "unwind")
}

fn is_static_lib_path(path: &str) -> bool {
    file_name(Path::new(path)).ends_with(".a")
}

fn is_shared_lib_path(path: &str) -> bool {
    let name = file_name(Path::new(path));
    if name.ends_with(".so") {
        return true;
    }
    let Some((_, suffix)) = name.split_once(".so.") else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn is_target_output(path: &str, outputs: &[PathBuf], implicit_outputs: &[PathBuf]) -> bool {
    let candidate = Path::new(path);
    outputs
        .iter()
        .chain(implicit_outputs.iter())
        .any(|out| out == candidate)
}

fn push_flag_library(info: &mut CcLinkInfo, state: Option<LibraryKind>, lib: PathBuf) {
    match state {
        Some(LibraryKind::Static) => info.libs_static.push(lib),
        Some(LibraryKind::StaticWhole) => info.libs_static_whole.push(lib),
        Some(LibraryKind::Shared) | None => info.libs_shared.push(lib),
        Some(LibraryKind::Unspecified) => (),
    }
}

fn handle_wl_flag(
    wl_arg: &str,
    full_flag: &str,
    info: &mut CcLinkInfo,
    state: &mut Option<LibraryKind>,
    prev_state: &mut Option<LibraryKind>,
    next_is_version_script: &mut bool,
) {
    match wl_arg {
        "-Bstatic" => {
            *state = Some(LibraryKind::Static);
            return;
        }
        "-Bdynamic" => {
            *state = Some(LibraryKind::Shared);
            return;
        }
        "--whole-archive" => {
            *prev_state = *state;
            *state = Some(LibraryKind::StaticWhole);
            return;
        }
        "--no-whole-archive" => {
            *state = *prev_state;
            return;
        }
        "--start-group" | "--end-group" => return,
        "--version-script" => *next_is_version_script = true,
        _ => {
            if let Some(vs) = wl_arg
                .strip_prefix("--version-script=")
                .or_else(|| wl_arg.strip_prefix("--version-script,"))
            {
                info.version_script = Some(PathBuf::from(vs));
            } else if *next_is_version_script {
                *next_is_version_script = false;
                info.version_script = Some(PathBuf::from(wl_arg));
            }
        }
    }
    info.link_flags.push(String::from(full_flag));
}

fn try_skip_link_arg(arg: &str, iter: &mut Peekable<Iter<String>>) -> bool {
    if arg == "-o" || arg == "-L" || is_depfile_flag_with_arg(arg) {
        iter.next();
        return true;
    }
    is_depfile_flag_standalone(arg)
        || arg == "-shared"
        || arg == "-pthread"
        || arg.starts_with("-L")
        || is_attached_output_flag(arg)
}

fn try_parse_link_library(
    arg: &str,
    iter: &mut Peekable<Iter<String>>,
    state: Option<LibraryKind>,
    outputs: &[PathBuf],
    implicit_outputs: &[PathBuf],
    link_info: &mut CcLinkInfo,
) -> bool {
    let flag_lib = if arg == "-l" {
        iter.next().map(String::as_str)
    } else {
        arg.strip_prefix("-l")
    };
    if let Some(library) = flag_lib {
        if !library.is_empty() && !is_system_lib(library) {
            push_flag_library(link_info, state, PathBuf::from(format!("lib{library}")));
        }
        return true;
    }
    if arg.starts_with('-') {
        return false;
    }
    if is_static_lib_path(arg) {
        if !is_target_output(arg, outputs, implicit_outputs) {
            if state == Some(LibraryKind::StaticWhole) {
                link_info.libs_static_whole.push(PathBuf::from(arg));
            } else {
                link_info.libs_static.push(PathBuf::from(arg));
            }
        }
        return true;
    }
    if is_shared_lib_path(arg) {
        if !is_target_output(arg, outputs, implicit_outputs) {
            link_info.libs_shared.push(PathBuf::from(arg));
        }
        return true;
    }
    false
}

pub fn parse_cc_link_args(
    tokens: &[String],
    outputs: &[PathBuf],
    implicit_outputs: &[PathBuf],
) -> (CcCompileInfo, CcLinkInfo) {
    let mut compile_info = CcCompileInfo::default();
    let mut link_info = CcLinkInfo::default();
    let mut state: Option<LibraryKind> = None;
    let mut prev_state: Option<LibraryKind> = None;
    let mut next_is_version_script = false;
    let mut iter = tokens.iter().peekable();

    while let Some(arg) = iter.next() {
        if try_skip_link_arg(arg, &mut iter)
            || try_parse_define_or_include(arg, &mut iter, &mut compile_info)
        {
            continue;
        }
        if arg == "-Xlinker" {
            if let Some(next) = iter.next() {
                if !next.starts_with("--dependency-file=") {
                    if next.contains(',') {
                        next_is_version_script = false;
                        link_info.link_flags.push(arg.clone());
                        link_info.link_flags.push(next.clone());
                    } else {
                        let wl_flag = format!("-Wl,{next}");
                        handle_wl_flag(
                            next,
                            &wl_flag,
                            &mut link_info,
                            &mut state,
                            &mut prev_state,
                            &mut next_is_version_script,
                        );
                    }
                }
            }
            continue;
        }
        if let Some(wl_arg) = arg.strip_prefix("-Wl,") {
            handle_wl_flag(
                wl_arg,
                arg,
                &mut link_info,
                &mut state,
                &mut prev_state,
                &mut next_is_version_script,
            );
            continue;
        }
        next_is_version_script = false;
        if !try_parse_link_library(
            arg,
            &mut iter,
            state,
            outputs,
            implicit_outputs,
            &mut link_info,
        ) && arg.starts_with('-')
        {
            link_info.link_flags.push(arg.clone());
        }
    }
    (compile_info, link_info)
}

pub fn parse_archive_libs(
    tokens: &[String],
    outputs: &[PathBuf],
    implicit_outputs: &[PathBuf],
) -> CcLinkInfo {
    let mut info = CcLinkInfo::default();
    for arg in tokens {
        if !arg.starts_with('-')
            && is_static_lib_path(arg)
            && !is_target_output(arg, outputs, implicit_outputs)
        {
            info.libs_static.push(PathBuf::from(arg));
        }
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn opencl_c_compile_detection() {
        assert!(is_opencl_c_compile(&vec_str(&["-x", "cl", "-c", "a.cl"])));
        assert!(is_opencl_c_compile(&vec_str(&["-xcl", "-c", "a.cl"])));
        assert!(!is_opencl_c_compile(&vec_str(&[
            "-x", "c++", "-c", "a.cpp"
        ])));
    }

    #[test]
    fn strip_depfile_flags_removes_all_variants_and_preserves_quoting() {
        assert_eq!(
            strip_depfile_flags(
                r#"clang -x cl -MD -MMD -MP -MT a.o -MQ a.o -MF a.d -MFb.d '-DFOO="a b"' -DAPI=__attribute__\(\(packed\)\) -c -o a.o a.cl"#
            ),
            r#"clang -x cl '-DFOO="a b"' -DAPI=__attribute__\(\(packed\)\) -c -o a.o a.cl"#
        );
    }

    #[test]
    fn parse_cc_compile_args_extracts_defines_includes_and_cflags() {
        let info = parse_cc_compile_args(&vec_str(&[
            "-c",
            "-oout.o",
            "-MD",
            "-MF",
            "out.d",
            "-DFOO=1",
            "-D",
            r#"STR="hi""#,
            "-Iinc1",
            "-I",
            "inc2",
            "-isystem",
            "sys1",
            "-isystemsys2",
            "-isystem",
            "-Iinc3",
            "-std=c11",
            "-Wall",
            "-objcmt-migrate-literals",
            "-offload-arch=sm_80",
            "src.c",
        ]));
        assert_eq!(info.defines, vec_str(&["FOO=1", r#"STR=\"hi\""#]));
        assert_eq!(
            info.includes,
            vec_str(&["inc1", "inc2", "sys1", "sys2", "inc3"])
        );
        assert_eq!(
            info.cflags,
            vec_str(&[
                "-std=c11",
                "-Wall",
                "-objcmt-migrate-literals",
                "-offload-arch=sm_80"
            ])
        );
        assert_eq!(info.c_std, Some(String::from("c11")));
    }

    #[test]
    fn shared_lib_path_matches_unversioned_and_numeric_sonames_only() {
        for path in ["libfoo.so", "dir/libfoo.so.1", "libfoo.so.1.2.3"] {
            assert!(is_shared_lib_path(path), "expected shared lib: {path}");
        }
        for path in [
            "libfoo.so.TOC",
            "libfoo.so.d",
            "libfoo.so.rsp",
            "libfoo.so.",
            "libfoo.so.1a",
            "libfoo.a",
        ] {
            assert!(!is_shared_lib_path(path), "unexpected shared lib: {path}");
        }
    }

    #[test]
    fn parse_cc_link_args_tracks_library_states_and_version_scripts() {
        let outputs = vec![PathBuf::from("libout.so")];
        let implicit_outputs = vec![PathBuf::from("libout.so.TOC")];
        let (compile_info, link_info) = parse_cc_link_args(
            &vec_str(&[
                "-shared",
                "-o",
                "libout.so",
                "libout.so",
                "libout.so.TOC",
                "-DFOO=1",
                "-L",
                "/usr/lib",
                "-L/opt/lib",
                "-Xlinker",
                "--dependency-file=libout.d",
                "-Xlinker",
                "-z",
                "-Wl,--start-group",
                "-Wl,-Bstatic",
                "-lstat1",
                "-Wl,--whole-archive",
                "libwhole.a",
                "-lwhole_flag",
                "-Wl,--no-whole-archive",
                "-lstat2",
                "-Wl,-Bdynamic",
                "-ldyn",
                "-ldl",
                "-lm",
                "-lc",
                "-lpthread",
                "-latomic",
                "-lunwind",
                "-pthread",
                "-Wl,--end-group",
                "libplain.a",
                "libver.so.1.2",
                "-Wl,--version-script,exports.map",
            ]),
            &outputs,
            &implicit_outputs,
        );
        assert_eq!(compile_info.defines, vec_str(&["FOO=1"]));
        assert_eq!(
            link_info.libs_static,
            vec![
                PathBuf::from("libstat1"),
                PathBuf::from("libstat2"),
                PathBuf::from("libplain.a")
            ]
        );
        assert_eq!(
            link_info.libs_static_whole,
            vec![PathBuf::from("libwhole.a"), PathBuf::from("libwhole_flag")]
        );
        assert_eq!(
            link_info.libs_shared,
            vec![PathBuf::from("libdyn"), PathBuf::from("libver.so.1.2")]
        );
        assert_eq!(link_info.version_script, Some(PathBuf::from("exports.map")));
        assert_eq!(
            link_info.link_flags,
            vec_str(&["-Wl,-z", "-Wl,--version-script,exports.map"])
        );
    }

    #[test]
    fn parse_cc_link_args_supports_split_version_script_flag() {
        let (_, link_info) = parse_cc_link_args(
            &vec_str(&["-Wl,--version-script", "-Wl,split.map"]),
            &[],
            &[],
        );
        assert_eq!(link_info.version_script, Some(PathBuf::from("split.map")));
    }

    #[test]
    fn parse_cc_link_args_handles_xlinker_pairs_spaced_l_and_flag_guards() {
        let (_, link_info) = parse_cc_link_args(
            &vec_str(&[
                "-olibout.so",
                "-Xlinker",
                "-z",
                "-Xlinker",
                "relro",
                "-Xlinker",
                "--exclude-libs=libignored.a",
                "-Xlinker",
                "--start-group",
                "-Xlinker",
                "-Bstatic",
                "-lstat_xl",
                "-Xlinker",
                "--whole-archive",
                "libwhole_xl.a",
                "-Xlinker",
                "--no-whole-archive",
                "-Xlinker",
                "-Bdynamic",
                "-Xlinker",
                "--end-group",
                "-Xlinker",
                "--version-script",
                "-Xlinker",
                "xl.map",
                "--exclude-libs=libalso_ignored.a",
                "-fsanitize-blacklist=ignored.so",
                "-object_path_lto",
                "-l",
                "spaced",
            ]),
            &[],
            &[],
        );
        assert_eq!(link_info.libs_static, vec![PathBuf::from("libstat_xl")]);
        assert_eq!(
            link_info.libs_static_whole,
            vec![PathBuf::from("libwhole_xl.a")]
        );
        assert_eq!(link_info.libs_shared, vec![PathBuf::from("libspaced")]);
        assert_eq!(link_info.version_script, Some(PathBuf::from("xl.map")));
        assert_eq!(
            link_info.link_flags,
            vec_str(&[
                "-Wl,-z",
                "-Wl,relro",
                "-Wl,--exclude-libs=libignored.a",
                "-Wl,--version-script",
                "-Wl,xl.map",
                "--exclude-libs=libalso_ignored.a",
                "-fsanitize-blacklist=ignored.so",
                "-object_path_lto",
            ])
        );
    }

    #[test]
    fn parse_archive_libs_excludes_target_outputs() {
        let info = parse_archive_libs(
            &vec_str(&["rcs", "libout.a", "obj.o", "libdep.a", "--flag=ignored.a"]),
            &[PathBuf::from("libout.a")],
            &[],
        );
        assert_eq!(info.libs_static, vec![PathBuf::from("libdep.a")]);
    }
}
