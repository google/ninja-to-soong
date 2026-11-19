// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::iter::Peekable;
use std::str;

use crate::ninja_target::*;
use crate::utils::*;

#[derive(Debug, Clone)]
struct NinjaRuleDef {
    command: String,
    rspfile: Option<String>,
    rspfile_content: Option<String>,
    description: Option<String>,
    generator: bool,
    variables: HashMap<String, String>,
}

#[derive(Default, Clone)]
struct NinjaScope {
    variables: HashMap<String, String>,
    rules: HashMap<String, NinjaRuleDef>,
}

fn split_unescaped_whitespace(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start: Option<usize> = None;
    let mut escaped = false;
    for (idx, ch) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '$' {
            start.get_or_insert(idx);
            escaped = true;
        } else if ch.is_ascii_whitespace() {
            if let Some(tok_start) = start.take() {
                parts.push(&s[tok_start..idx]);
            }
        } else {
            start.get_or_insert(idx);
        }
    }
    if let Some(tok_start) = start {
        parts.push(&s[tok_start..]);
    }
    parts
}

fn parse_paths(s: &str, vars: &HashMap<String, String>) -> Vec<PathBuf> {
    let mut lookup = |name: &str| vars.get(name).cloned();
    split_unescaped_whitespace(s)
        .into_iter()
        .map(|tok| expand_ninja_string(tok, &mut lookup, false))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn parse_output_section(
    section: &str,
    vars: &HashMap<String, String>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    match section.split_once('|') {
        Some((outputs, implicit_outs)) => {
            (parse_paths(outputs, vars), parse_paths(implicit_outs, vars))
        }
        None => (parse_paths(section, vars), Vec::new()),
    }
}

fn parse_input_and_rule_section(
    section: &str,
    vars: &HashMap<String, String>,
) -> (String, Vec<PathBuf>) {
    let section = section.trim();
    match section.split_once(|c: char| c.is_ascii_whitespace()) {
        Some((rule, inputs)) => (String::from(rule), parse_paths(inputs, vars)),
        None => (String::from(section), Vec::new()),
    }
}

fn parse_input_section(
    section: &str,
    vars: &HashMap<String, String>,
) -> (String, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let (input_and_deps, order_only_deps) = match section.split_once("||") {
        Some((head, tail)) => (head, parse_paths(tail, vars)),
        None => (section, Vec::new()),
    };
    let (input_and_rule, implicit_deps) = match input_and_deps.split_once('|') {
        Some((head, tail)) => (head, parse_paths(tail, vars)),
        None => (input_and_deps, Vec::new()),
    };
    let (rule, inputs) = parse_input_and_rule_section(input_and_rule, vars);
    (rule, inputs, implicit_deps, order_only_deps)
}

fn find_colon_index(line: &str) -> Option<usize> {
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '$' {
            escaped = true;
        } else if ch == ':' {
            return Some(idx);
        }
    }
    None
}

fn split_output_and_input_sections(line: &str) -> Result<(&str, &str), String> {
    let Some(index) = find_colon_index(line) else {
        return error!("split_output_and_input_sections failed: '{line}'");
    };
    Ok((&line[..index], &line[index + 1..]))
}

fn parse_key_value(line: &str) -> Result<(String, String), String> {
    let Some((key, value)) = line.split_once('=') else {
        return error!("parse_key_value failed: '{line}'");
    };
    Ok((String::from(key.trim()), String::from(value.trim())))
}

fn parse_indented_variables(
    lines: &mut Peekable<str::Lines>,
) -> Result<HashMap<String, String>, String> {
    let mut variables = HashMap::new();
    while let Some(next_line) = lines.next_if(|line| line.starts_with(' ')) {
        let (key, value) = parse_key_value(next_line)?;
        variables.insert(key, value);
    }
    Ok(variables)
}

fn is_ninja_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn read_braced_var_name(chars: &mut Peekable<str::Chars>) -> String {
    let mut name = String::new();
    for ch in chars.by_ref() {
        if ch == '}' {
            break;
        }
        name.push(ch);
    }
    name
}

fn read_bare_var_name(chars: &mut Peekable<str::Chars>) -> String {
    let mut name = String::new();
    while let Some(&ch) = chars.peek() {
        if !is_ninja_ident_char(ch) {
            break;
        }
        name.push(ch);
        chars.next();
    }
    name
}

fn append_expanded_var(
    name: &str,
    lookup: &mut impl FnMut(&str) -> Option<String>,
    preserve_rspfile: bool,
    out: &mut String,
) {
    if preserve_rspfile && name == "rspfile" {
        out.push_str("${rspfile}");
    } else if let Some(val) = lookup(name) {
        out.push_str(&val);
    }
}

fn expand_ninja_string(
    template: &str,
    lookup: &mut impl FnMut(&str) -> Option<String>,
    preserve_rspfile: bool,
) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' {
            result.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some(escaped @ ('$' | ' ' | ':')) => {
                chars.next();
                result.push(escaped);
            }
            Some('{') => {
                chars.next();
                let name = read_braced_var_name(&mut chars);
                append_expanded_var(&name, lookup, preserve_rspfile, &mut result);
            }
            Some(c) if is_ninja_ident_char(c) => {
                let name = read_bare_var_name(&mut chars);
                append_expanded_var(&name, lookup, preserve_rspfile, &mut result);
            }
            _ => result.push('$'),
        }
    }
    result
}

fn join_paths(paths: &[PathBuf], sep: &str) -> String {
    paths
        .iter()
        .map(path_to_string)
        .collect::<Vec<_>>()
        .join(sep)
}

fn expand_edge_variables(
    raw_vars: HashMap<String, String>,
    globals: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut lookup = |name: &str| globals.get(name).cloned();
    raw_vars
        .into_iter()
        .map(|(key, val)| (key, expand_ninja_string(&val, &mut lookup, false)))
        .collect()
}

struct RuleLookupContext<'a> {
    in_str: &'a str,
    in_newline_str: &'a str,
    out_str: &'a str,
    edge_vars: &'a HashMap<String, String>,
    rule_vars: &'a HashMap<String, String>,
    scope_vars: &'a HashMap<String, String>,
}

impl RuleLookupContext<'_> {
    fn lookup(&self, name: &str, visiting: &mut Vec<String>) -> Option<String> {
        match name {
            "in" => Some(String::from(self.in_str)),
            "in_newline" => Some(String::from(self.in_newline_str)),
            "out" => Some(String::from(self.out_str)),
            _ => {
                if let Some(val) = self.edge_vars.get(name) {
                    return Some(val.clone());
                }
                if let Some(tmpl) = self.rule_vars.get(name) {
                    if visiting.iter().any(|v| v == name) {
                        return None;
                    }
                    visiting.push(String::from(name));
                    let expanded = self.expand_with_visiting(tmpl, false, visiting);
                    visiting.pop();
                    return Some(expanded);
                }
                self.scope_vars.get(name).cloned()
            }
        }
    }

    fn expand_with_visiting(
        &self,
        template: &str,
        preserve_rspfile: bool,
        visiting: &mut Vec<String>,
    ) -> String {
        expand_ninja_string(
            template,
            &mut |name| self.lookup(name, visiting),
            preserve_rspfile,
        )
    }

    fn expand(&self, template: &str, preserve_rspfile: bool) -> String {
        self.expand_with_visiting(template, preserve_rspfile, &mut Vec::new())
    }
}

fn resolve_rule_command(
    common: &NinjaTargetCommon,
    edge_vars: &HashMap<String, String>,
    scope: &NinjaScope,
) -> Option<ResolvedRuleCommand> {
    let rule_def = scope.rules.get(&common.rule)?;
    let in_str = join_paths(&common.inputs, " ");
    let in_newline_str = join_paths(&common.inputs, "\n");
    let out_str = join_paths(&common.outputs, " ");
    let ctx = RuleLookupContext {
        in_str: &in_str,
        in_newline_str: &in_newline_str,
        out_str: &out_str,
        edge_vars,
        rule_vars: &rule_def.variables,
        scope_vars: &scope.variables,
    };

    let rspfile = rule_def
        .rspfile
        .as_deref()
        .map(|tmpl| ctx.expand(tmpl, false));
    let rspfile_content = rule_def
        .rspfile_content
        .as_deref()
        .map(|tmpl| ctx.expand(tmpl, false));
    let rsp_info = rspfile.zip(rspfile_content);

    let command = ctx.expand(&rule_def.command, rsp_info.is_some());
    let description = rule_def
        .description
        .as_deref()
        .map(|tmpl| ctx.expand(tmpl, false));
    let is_generator = rule_def.generator || edge_vars.contains_key("generator");

    Some(ResolvedRuleCommand {
        command,
        rsp_info,
        description,
        is_generator,
    })
}

fn parse_build_target(
    line: &str,
    lines: &mut Peekable<str::Lines>,
    scope: &NinjaScope,
) -> Result<Vec<NinjaTarget>, String> {
    let Some(line_stripped) = line.strip_prefix("build ") else {
        return error!("parse_build_target failed: '{line}'");
    };
    let (output_section, input_section) = split_output_and_input_sections(line_stripped)?;
    let (outputs, implicit_outputs) = parse_output_section(output_section, &scope.variables);
    let (rule, inputs, implicit_deps, order_only_deps) =
        parse_input_section(input_section, &scope.variables);
    let raw_edge_vars = parse_indented_variables(lines)?;
    let edge_vars = expand_edge_variables(raw_edge_vars, &scope.variables);

    let common = NinjaTargetCommon {
        rule,
        outputs,
        implicit_outputs,
        inputs,
        implicit_deps,
        order_only_deps,
    };
    let resolved = resolve_rule_command(&common, &edge_vars, scope);
    let target = NinjaTarget::new(common, resolved);
    Ok(target.split_multi_output())
}

fn parse_ninja_rule(
    line: &str,
    lines: &mut Peekable<str::Lines>,
) -> Result<(String, NinjaRuleDef), String> {
    let Some(rule) = line.strip_prefix("rule ") else {
        return error!("parse_ninja_rule failed: '{line}'");
    };
    let mut vars = parse_indented_variables(lines)?;
    let Some(command) = vars.remove("command") else {
        return error!("parse_ninja_rule failed: missing command in rule '{rule}'");
    };
    Ok((
        String::from(rule.trim()),
        NinjaRuleDef {
            command,
            rspfile: vars.get("rspfile").cloned(),
            rspfile_content: vars.get("rspfile_content").cloned(),
            description: vars.get("description").cloned(),
            generator: vars.contains_key("generator"),
            variables: vars,
        },
    ))
}

fn parse_referenced_file_path(
    line: &str,
    build_path: &Path,
    vars: &HashMap<String, String>,
) -> Result<PathBuf, String> {
    let Some((_, file)) = line.split_once(' ') else {
        return error!("parse_referenced_file_path failed: '{line}'");
    };
    let mut lookup = |name: &str| vars.get(name).cloned();
    let expanded = expand_ninja_string(file.trim(), &mut lookup, false);
    Ok(build_path.join(expanded))
}

fn parse_ninja_file_into_scope(
    file_path: PathBuf,
    build_path: &Path,
    scope: &mut NinjaScope,
    targets: &mut Vec<NinjaTarget>,
) -> Result<(), String> {
    let file = read_file(&file_path)?.replace("$\n", " ");
    let mut lines = file.lines().peekable();

    while let Some(line) = lines.next() {
        if line.is_empty()
            || line.starts_with("default ")
            || line.starts_with("pool ")
            || line.starts_with('#')
            || line.starts_with(' ')
        {
            continue;
        } else if line.starts_with("rule ") {
            let (name, rule_def) = parse_ninja_rule(line, &mut lines)?;
            scope.rules.insert(name, rule_def);
        } else if line.starts_with("build ") {
            targets.extend(parse_build_target(line, &mut lines, scope)?);
        } else if line.starts_with("include ") {
            let path = parse_referenced_file_path(line, build_path, &scope.variables)?;
            parse_ninja_file_into_scope(path, build_path, scope, targets)?;
        } else if line.starts_with("subninja ") {
            let path = parse_referenced_file_path(line, build_path, &scope.variables)?;
            let mut child_scope = scope.clone();
            parse_ninja_file_into_scope(path, build_path, &mut child_scope, targets)?;
        } else {
            let (key, raw_value) = parse_key_value(line)?;
            let mut lookup = |name: &str| scope.variables.get(name).cloned();
            let value = expand_ninja_string(&raw_value, &mut lookup, false);
            scope.variables.insert(key, value);
        }
    }
    Ok(())
}

fn convert_custom_input_archives(targets: &mut [NinjaTarget]) {
    let custom_cmd_outputs: HashSet<PathBuf> = targets
        .iter()
        .filter(|target| target.is_custom_command())
        .flat_map(|target| {
            target
                .get_outputs()
                .iter()
                .chain(target.get_implicit_outputs().iter())
                .cloned()
        })
        .collect();

    for target in targets {
        target.convert_archive_to_custom_command_if_needed(&custom_cmd_outputs);
    }
}

pub fn parse_build_ninja(build_path: &Path) -> Result<Vec<NinjaTarget>, String> {
    let mut scope = NinjaScope::default();
    let mut targets = Vec::new();
    parse_ninja_file_into_scope(
        build_path.join("build.ninja"),
        build_path,
        &mut scope,
        &mut targets,
    )?;
    convert_custom_input_archives(&mut targets);
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_ninja_string_handles_variables_and_escapes() {
        let mut lookup = |name: &str| -> Option<String> {
            match name {
                "foo" => Some(String::from("BAR")),
                "a-b_1" => Some(String::from("OK")),
                "rspfile" => Some(String::from("out.rsp")),
                _ => None,
            }
        };
        assert_eq!(
            expand_ninja_string("$foo/${foo}_${a-b_1}_$missing $$ $ $:", &mut lookup, false),
            "BAR/BAR_OK_ $  :"
        );
        assert_eq!(
            expand_ninja_string("@$rspfile @${rspfile}", &mut lookup, true),
            "@${rspfile} @${rspfile}"
        );
        assert_eq!(
            expand_ninja_string("@$rspfile @${rspfile}", &mut lookup, false),
            "@out.rsp @out.rsp"
        );
    }

    #[test]
    fn find_colon_index_skips_escaped_colons_but_not_escaped_dollars() {
        assert_eq!(find_colon_index("a$:b: rule in"), Some(4));
        assert_eq!(find_colon_index("foo$$: rule in"), Some(5));
        assert_eq!(find_colon_index("a$:b$:c"), None);
    }

    #[test]
    fn parse_output_input_and_referenced_paths_expand_vars_and_escapes() {
        let vars: HashMap<String, String> = [
            (String::from("out_dir"), String::from("build")),
            (String::from("src_dir"), String::from("src")),
        ]
        .into_iter()
        .collect();

        let (outs, imp_outs) =
            parse_output_section("$out_dir/a.so foo$ bar.so a$:b | $out_dir/a.so.TOC", &vars);
        assert_eq!(
            outs,
            vec![
                PathBuf::from("build/a.so"),
                PathBuf::from("foo bar.so"),
                PathBuf::from("a:b"),
            ]
        );
        assert_eq!(imp_outs, vec![PathBuf::from("build/a.so.TOC")]);

        let (rule, ins, imp_deps, order_deps) = parse_input_section(
            " link $out_dir/a.o $src_dir/b.o | $out_dir/libdep.a || gen$ 1.h",
            &vars,
        );
        assert_eq!(rule, "link");
        assert_eq!(
            ins,
            vec![PathBuf::from("build/a.o"), PathBuf::from("src/b.o")]
        );
        assert_eq!(imp_deps, vec![PathBuf::from("build/libdep.a")]);
        assert_eq!(order_deps, vec![PathBuf::from("gen 1.h")]);

        let sub =
            parse_referenced_file_path("subninja $src_dir/child.ninja", Path::new("/b"), &vars)
                .unwrap();
        assert_eq!(sub, PathBuf::from("/b/src/child.ninja"));
    }

    #[test]
    fn resolve_rule_command_expands_rule_variables_with_edge_override_and_cycle_guard() {
        let mut scope = NinjaScope::default();
        scope
            .variables
            .insert(String::from("flags"), String::from("-O2"));
        scope.rules.insert(
            String::from("cc"),
            NinjaRuleDef {
                command: String::from("clang -MMD -MF $depfile $flags -c $in -o $out $cyc"),
                rspfile: None,
                rspfile_content: None,
                description: None,
                generator: false,
                variables: [
                    (String::from("depfile"), String::from("$out.d")),
                    (String::from("cyc"), String::from("$cyc")),
                ]
                .into_iter()
                .collect(),
            },
        );
        let common = NinjaTargetCommon {
            rule: String::from("cc"),
            outputs: vec![PathBuf::from("a.o")],
            implicit_outputs: Vec::new(),
            inputs: vec![PathBuf::from("a.c")],
            implicit_deps: Vec::new(),
            order_only_deps: Vec::new(),
        };

        let resolved = resolve_rule_command(&common, &HashMap::new(), &scope).unwrap();
        assert_eq!(resolved.command, "clang -MMD -MF a.o.d -O2 -c a.c -o a.o ");

        let edge_vars: HashMap<String, String> =
            [(String::from("depfile"), String::from("custom.d"))]
                .into_iter()
                .collect();
        let overridden = resolve_rule_command(&common, &edge_vars, &scope).unwrap();
        assert_eq!(
            overridden.command,
            "clang -MMD -MF custom.d -O2 -c a.c -o a.o "
        );
    }
}
