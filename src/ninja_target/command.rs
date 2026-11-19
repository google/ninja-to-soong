// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::iter::Peekable;
use std::str::Chars;

use crate::utils::*;

fn consume_single_quoted(chars: &mut Peekable<Chars>, out: &mut String) {
    for ch in chars.by_ref() {
        if ch == '\'' {
            break;
        }
        out.push(ch);
    }
}

fn consume_double_quoted(chars: &mut Peekable<Chars>, out: &mut String) {
    while let Some(ch) = chars.next() {
        if ch == '"' {
            break;
        }
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if matches!(next, '"' | '\\' | '$' | '`') {
                    out.push(chars.next().unwrap());
                    continue;
                }
            }
        }
        out.push(ch);
    }
}

pub fn unquote_shell_token(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => consume_single_quoted(&mut chars, &mut out),
            '"' => consume_double_quoted(&mut chars, &mut out),
            '\\' => {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

fn skip_quoted_bytes(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if quote == b'"' && bytes[i] == b'\\' {
            i = (i + 2).min(bytes.len());
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn scan_raw_shell_token_end(input: &str) -> usize {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = skip_quoted_bytes(bytes, i, b'\''),
            b'"' => i = skip_quoted_bytes(bytes, i, b'"'),
            b'\\' => i = (i + 2).min(bytes.len()),
            b if b.is_ascii_whitespace() => break,
            _ => i += 1,
        }
    }
    i
}

pub fn split_first_raw_shell_token(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start_matches(|c: char| c.is_ascii_whitespace());
    if trimmed.is_empty() {
        return None;
    }
    let end = scan_raw_shell_token_end(trimmed);
    let rest = trimmed[end..].trim_start_matches(|c: char| c.is_ascii_whitespace());
    Some((&trimmed[..end], rest))
}

pub fn split_raw_shell_tokens(mut input: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    while let Some((token, rest)) = split_first_raw_shell_token(input) {
        tokens.push(token);
        input = rest;
    }
    tokens
}

pub fn split_shell_tokens(input: &str) -> Vec<String> {
    split_raw_shell_tokens(input)
        .into_iter()
        .map(unquote_shell_token)
        .collect()
}

fn copy_single_quoted_raw(chars: &mut Peekable<Chars>, out: &mut String) {
    out.push('\'');
    for ch in chars.by_ref() {
        out.push(ch);
        if ch == '\'' {
            break;
        }
    }
}

fn copy_double_quoted_raw(chars: &mut Peekable<Chars>, out: &mut String) {
    out.push('"');
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else if ch == '"' {
            break;
        }
    }
}

pub fn split_command_chain(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut paren_depth: usize = 0;
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                current.push(ch);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '\'' => copy_single_quoted_raw(&mut chars, &mut current),
            '"' => copy_double_quoted_raw(&mut chars, &mut current),
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            '&' if paren_depth == 0 && chars.peek() == Some(&'&') => {
                chars.next();
                segments.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let tail = current.trim();
    if !tail.is_empty() {
        segments.push(tail.to_string());
    }
    segments
}

fn exe_basename(token: &str) -> String {
    file_name(Path::new(token))
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExtractedCommand {
    BuildTool(String),
    Custom(String),
}

fn is_ranlib_or_strip(exe: &str) -> bool {
    let exe = strip_version_suffix(exe);
    matches!(exe, "ranlib" | "llvm-ranlib" | "strip" | "llvm-strip")
        || exe.ends_with("-ranlib")
        || exe.ends_with("-strip")
}

fn is_boilerplate_segment(segment: &str) -> bool {
    let tokens = split_shell_tokens(segment);
    let Some(first) = tokens.first() else {
        return true;
    };
    let exe = exe_basename(first);
    if matches!(exe.as_str(), ":" | "true" | "cd" | "rm") || is_ranlib_or_strip(&exe) {
        return true;
    }
    exe == "cmake"
        && tokens.get(1).map(String::as_str) == Some("-E")
        && matches!(
            tokens.get(2).map(String::as_str),
            Some("rm" | "cmake_transform_depfile")
        )
}

fn strip_compiler_launcher(mut command: &str) -> &str {
    while let Some((first_raw, rest)) = split_first_raw_shell_token(command) {
        if rest.is_empty() {
            break;
        }
        let exe = exe_basename(&unquote_shell_token(first_raw));
        if matches!(exe.as_str(), "ccache" | "sccache") {
            command = rest;
        } else {
            break;
        }
    }
    command
}

fn extract_capture_output(tokens: &[String]) -> Option<String> {
    let idx = tokens.iter().position(|tok| tok == "--capture")?;
    tokens.get(idx + 1).cloned()
}

fn is_meson_internal_exe(tokens: &[String]) -> bool {
    tokens
        .windows(2)
        .any(|pair| pair[0] == "--internal" && pair[1] == "exe")
}

fn unwrap_command_wrapper(command: &str) -> String {
    let command = strip_compiler_launcher(command.trim());
    let Some((wrapper, inner)) = command.split_once(" -- ") else {
        return String::from(command);
    };
    let wrapper_tokens = split_shell_tokens(wrapper);
    if is_meson_internal_exe(&wrapper_tokens) {
        if let Some(output) = extract_capture_output(&wrapper_tokens) {
            return format!("{inner} > {output}");
        }
        return String::from(inner);
    }
    if let Some(first_inner) = split_shell_tokens(inner).first() {
        if is_c_cpp_compiler(&exe_basename(first_inner)) {
            return String::from(inner);
        }
    }
    String::from(command)
}

fn is_known_build_tool(exe: &str) -> bool {
    is_c_cpp_compiler(exe) || is_archiver(exe) || is_rustc(exe) || is_bindgen(exe)
}

pub fn extract_command(command: &str, description: Option<&str>) -> Option<ExtractedCommand> {
    let mut custom_segments = Vec::new();
    for segment in split_command_chain(command) {
        if is_boilerplate_segment(&segment) {
            continue;
        }
        let unwrapped = unwrap_command_wrapper(&segment);
        let tokens = split_shell_tokens(&unwrapped);
        let Some(first_tok) = tokens.first() else {
            continue;
        };
        if is_known_build_tool(&exe_basename(first_tok)) {
            return Some(ExtractedCommand::BuildTool(unwrapped));
        }
        if !is_ignored_command(&unwrapped, &tokens, description) {
            custom_segments.push(unwrapped);
        }
    }
    if custom_segments.is_empty() {
        None
    } else {
        Some(ExtractedCommand::Custom(custom_segments.join(" && ")))
    }
}

pub fn expand_rspfile_tokens(
    tokens: Vec<String>,
    rsp_info: Option<&(String, String)>,
) -> Vec<String> {
    let Some((rsp_file, rsp_content)) = rsp_info else {
        return tokens;
    };
    let mut expanded = Vec::with_capacity(tokens.len());
    for token in tokens {
        if let Some(path) = token.strip_prefix('@') {
            if path == rsp_file || path == "${rspfile}" {
                expanded.extend(split_shell_tokens(rsp_content));
                continue;
            }
        }
        expanded.push(token);
    }
    expanded
}

fn strip_version_suffix(exe: &str) -> &str {
    if let Some((prefix, suffix)) = exe.rsplit_once('-') {
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return prefix;
        }
    }
    exe
}

pub fn is_c_cpp_compiler(exe: &str) -> bool {
    let exe = strip_version_suffix(exe);
    matches!(exe, "cc" | "c++" | "gcc" | "g++" | "clang" | "clang++")
        || exe.ends_with("-gcc")
        || exe.ends_with("-g++")
        || exe.ends_with("-clang")
        || exe.ends_with("-clang++")
}

pub fn is_archiver(exe: &str) -> bool {
    let exe = strip_version_suffix(exe);
    matches!(exe, "ar" | "llvm-ar") || exe.ends_with("-ar")
}

pub fn is_rustc(exe: &str) -> bool {
    exe == "rustc"
}

pub fn is_bindgen(exe: &str) -> bool {
    exe == "bindgen"
}

fn has_shell_redirection(command: &str) -> bool {
    let bytes = command.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = skip_quoted_bytes(bytes, i, b'\''),
            b'"' => i = skip_quoted_bytes(bytes, i, b'"'),
            b'\\' => i = (i + 2).min(bytes.len()),
            b'>' => return true,
            _ => i += 1,
        }
    }
    false
}

fn is_cmake_generating_command(tokens: &[String], description: Option<&str>) -> bool {
    let has_script_or_copy = tokens.iter().any(|tok| tok == "-P")
        || tokens
            .windows(2)
            .any(|pair| pair[0] == "-E" && pair[1] == "copy_if_different");
    has_script_or_copy && description.is_some_and(|desc| desc.starts_with("Generating "))
}

pub fn is_ignored_command(command: &str, tokens: &[String], description: Option<&str>) -> bool {
    let Some(first) = tokens.first() else {
        return true;
    };
    let exe = exe_basename(first);
    if matches!(
        exe.as_str(),
        ":" | "true" | "touch" | "mkdir" | "rm" | "ln" | "ninja" | "cpack"
    ) || is_ranlib_or_strip(&exe)
    {
        return true;
    }
    if exe == "echo" && !has_shell_redirection(command) {
        return true;
    }
    if exe == "meson"
        && tokens.iter().any(|tok| tok == "--internal")
        && !is_meson_internal_exe(tokens)
    {
        return true;
    }
    if exe == "cmake" && !is_cmake_generating_command(tokens, description) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn split_shell_tokens_handles_quotes_and_escapes() {
        assert_eq!(
            split_shell_tokens(r#"clang '-DFOO="a b"' "-DBAR=\"c\"" -DBAZ=\(1\) "" ''"#),
            vec_str(&[
                "clang",
                r#"-DFOO="a b""#,
                r#"-DBAR="c""#,
                "-DBAZ=(1)",
                "",
                ""
            ])
        );
    }

    #[test]
    fn split_shell_tokens_preserves_unknown_escapes_in_double_quotes() {
        assert_eq!(
            split_shell_tokens(r#""\n" "\$" "\\" "\"" "\`""#),
            vec_str(&[r#"\n"#, "$", "\\", "\"", "`"])
        );
    }

    #[test]
    fn split_command_chain_ignores_quoted_and_subshell_and() {
        assert_eq!(
            split_command_chain(r#"cd /tmp && (echo a && echo b) && tool "x && y" 'u && v' && : "#),
            vec_str(&[
                "cd /tmp",
                "(echo a && echo b)",
                r#"tool "x && y" 'u && v'"#,
                ":"
            ])
        );
    }

    #[test]
    fn extract_command_strips_boilerplate_and_prefers_build_tool() {
        assert_eq!(
            extract_command(
                ": && cd /tmp && rm -f out.a && llvm-ar-21 rcs out.a a.o && llvm-ranlib-21 out.a",
                None
            ),
            Some(ExtractedCommand::BuildTool(String::from(
                "llvm-ar-21 rcs out.a a.o"
            )))
        );
        assert_eq!(
            extract_command(
                "cmake -E copy_directory a b && python3 prep.py && clang++ -o app a.o && llvm-strip-21 app",
                None
            ),
            Some(ExtractedCommand::BuildTool(String::from(
                "clang++ -o app a.o"
            )))
        );
        assert_eq!(extract_command(": && true && rm -f tmp", None), None);
    }

    #[test]
    fn extract_command_unwraps_launchers_capture_and_wrappers() {
        assert_eq!(
            extract_command("ccache clang -c a.c -o a.o", None),
            Some(ExtractedCommand::BuildTool(String::from(
                "clang -c a.c -o a.o"
            )))
        );
        assert_eq!(
            extract_command("/usr/bin/ccache clang -c a.c -o a.o", None),
            Some(ExtractedCommand::BuildTool(String::from(
                "clang -c a.c -o a.o"
            )))
        );
        assert_eq!(
            extract_command(
                r#""/opt/my tools/sccache" aarch64-linux-android-clang++ -c a.cpp -o a.o"#,
                None
            ),
            Some(ExtractedCommand::BuildTool(String::from(
                "aarch64-linux-android-clang++ -c a.cpp -o a.o"
            )))
        );
        assert_eq!(
            extract_command(
                "meson --internal exe --capture out.h -- python3 gen.py",
                None
            ),
            Some(ExtractedCommand::Custom(String::from(
                "python3 gen.py > out.h"
            )))
        );
        assert_eq!(
            extract_command("meson --internal exe -- python3 gen.py out.h", None),
            Some(ExtractedCommand::Custom(String::from(
                "python3 gen.py out.h"
            )))
        );
        assert_eq!(
            extract_command("custom_tool --capture out.h -- subarg", None),
            Some(ExtractedCommand::Custom(String::from(
                "custom_tool --capture out.h -- subarg"
            )))
        );
        assert_eq!(
            extract_command(
                "python3 link.py --flag -- clang++ -shared -o lib.so a.o",
                None
            ),
            Some(ExtractedCommand::BuildTool(String::from(
                "clang++ -shared -o lib.so a.o"
            )))
        );
        assert_eq!(
            extract_command("bindgen wrapper.h -- -DFOO=1", None),
            Some(ExtractedCommand::BuildTool(String::from(
                "bindgen wrapper.h -- -DFOO=1"
            )))
        );
    }

    #[test]
    fn extract_command_skips_ignored_steps_and_keeps_multi_step_custom_commands() {
        assert_eq!(
            extract_command("mkdir -p gen && python3 gen.py in out", None),
            Some(ExtractedCommand::Custom(String::from(
                "python3 gen.py in out"
            )))
        );
        assert_eq!(
            extract_command(
                "cd /build && /usr/bin/cmake -E make_directory gen && python3 gen.py in out",
                None
            ),
            Some(ExtractedCommand::Custom(String::from(
                "python3 gen.py in out"
            )))
        );
        assert_eq!(
            extract_command(
                "mkdir -p gen && step1.sh in tmp && step2.sh tmp out && touch stamp",
                None
            ),
            Some(ExtractedCommand::Custom(String::from(
                "step1.sh in tmp && step2.sh tmp out"
            )))
        );
        assert_eq!(extract_command("mkdir -p gen && touch stamp", None), None);
    }

    #[test]
    fn expand_rspfile_tokens_expands_matching_path_and_placeholder() {
        let rsp = (String::from("foo.rsp"), String::from("-DA=1 '-DB=\"x y\"'"));
        assert_eq!(
            expand_rspfile_tokens(vec_str(&["clang", "@foo.rsp", "@other.rsp"]), Some(&rsp)),
            vec_str(&["clang", "-DA=1", r#"-DB="x y""#, "@other.rsp"])
        );
        assert_eq!(
            expand_rspfile_tokens(vec_str(&["ar", "rcs", "lib.a", "@${rspfile}"]), Some(&rsp)),
            vec_str(&["ar", "rcs", "lib.a", "-DA=1", r#"-DB="x y""#])
        );
    }

    #[test]
    fn compiler_and_archiver_detection() {
        for exe in [
            "cc",
            "c++",
            "gcc",
            "g++",
            "clang",
            "clang++",
            "clang-21",
            "gcc-14",
            "aarch64-linux-android35-clang++",
            "x86_64-linux-gnu-gcc-13",
        ] {
            assert!(is_c_cpp_compiler(exe), "expected compiler: {exe}");
        }
        for exe in [
            "python3",
            "rustc",
            "bindgen",
            "ar",
            "clang-format",
            "clang-tidy",
        ] {
            assert!(!is_c_cpp_compiler(exe), "unexpected compiler: {exe}");
        }
        for exe in [
            "ar",
            "llvm-ar",
            "llvm-ar-21",
            "aarch64-linux-android-ar",
            "aarch64-linux-gnu-ar-14",
        ] {
            assert!(is_archiver(exe), "expected archiver: {exe}");
        }
        assert!(!is_archiver("tar"));
        for exe in ["ranlib", "llvm-ranlib-21", "strip", "llvm-strip-21"] {
            assert!(is_ranlib_or_strip(exe), "expected ranlib/strip: {exe}");
        }
    }

    #[test]
    fn ignored_command_classification() {
        assert!(is_ignored_command("", &[], None));
        assert!(is_ignored_command(
            "touch stamp",
            &vec_str(&["touch", "stamp"]),
            None
        ));
        assert!(is_ignored_command(
            r#"echo "hello > world""#,
            &vec_str(&["echo", "hello > world"]),
            None
        ));
        assert!(!is_ignored_command(
            "echo '#define X 1' > config.h",
            &vec_str(&["echo", "#define X 1", ">", "config.h"]),
            None
        ));
        assert!(is_ignored_command(
            "/usr/bin/meson --internal symbolextractor a.so",
            &vec_str(&["/usr/bin/meson", "--internal", "symbolextractor", "a.so"]),
            None
        ));
        assert!(!is_ignored_command(
            "/usr/bin/meson --internal exe --unpickle f.dat",
            &vec_str(&["/usr/bin/meson", "--internal", "exe", "--unpickle", "f.dat"]),
            None
        ));
        assert!(is_ignored_command(
            "/usr/bin/cmake -E touch stamp",
            &vec_str(&["/usr/bin/cmake", "-E", "touch", "stamp"]),
            None
        ));
        assert!(is_ignored_command(
            "/usr/bin/cmake -P gen.cmake",
            &vec_str(&["/usr/bin/cmake", "-P", "gen.cmake"]),
            Some("Running script")
        ));
        assert!(!is_ignored_command(
            "/usr/bin/cmake -P gen.cmake",
            &vec_str(&["/usr/bin/cmake", "-P", "gen.cmake"]),
            Some("Generating out.h")
        ));
        assert!(!is_ignored_command(
            "/usr/bin/cmake -E copy_if_different a b",
            &vec_str(&["/usr/bin/cmake", "-E", "copy_if_different", "a", "b"]),
            Some("Generating b")
        ));
    }
}
