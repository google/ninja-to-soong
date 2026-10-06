// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::iter::Peekable;
use std::str;

use crate::ninja_target::*;
use crate::utils::*;

fn split_paths(s: &str) -> Vec<PathBuf> {
    s.trim()
        .split(" ")
        .map(|p| PathBuf::from(p.trim()))
        .collect()
}

fn parse_output_section(section: &str) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let (outputs, implicit_outputs) = match section.split_once("|") {
        Some((outputs, implicit_outs)) => (split_paths(outputs), split_paths(implicit_outs)),
        None => (split_paths(section), Vec::new()),
    };
    Ok((outputs, implicit_outputs))
}

fn parse_input_and_rule_section(section: &str) -> Result<(String, Vec<PathBuf>), String> {
    let section = section.trim();
    let (rule, inputs) = match section.split_once(" ") {
        Some((rule, inputs)) => (String::from(rule), split_paths(inputs)),
        None => (String::from(section), Vec::new()),
    };
    Ok((rule, inputs))
}

fn parse_input_section(
    section: &str,
) -> Result<(String, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>), String> {
    let (input_and_deps, order_only_deps) = match section.split_once("||") {
        Some((head, tail)) => (head, split_paths(tail)),
        None => (section, Vec::new()),
    };
    let (input_and_rule, implicit_deps) = match input_and_deps.split_once("|") {
        Some((head, tail)) => (head, split_paths(tail)),
        None => (input_and_deps, Vec::new()),
    };
    let (rule, inputs) = parse_input_and_rule_section(input_and_rule)?;
    Ok((rule, inputs, implicit_deps, order_only_deps))
}

fn find_column_index(line: &str) -> Option<usize> {
    let index = line.find(":")?;
    if line[..index].ends_with('$') {
        return Some(index + 1 + find_column_index(&line[index + 1..])?);
    }
    Some(index)
}

fn split_output_and_input_sections(line: &str) -> Result<(&str, &str), String> {
    let Some(index) = find_column_index(line) else {
        return error!("split_output_and_input_sections failed: '{line}'");
    };
    Ok((&line[..index], &line[index + 1..]))
}

fn parse_key_value(line: &str) -> Result<(String, String), String> {
    let Some((key, value)) = line.split_once("=") else {
        return error!("parse_key_value failed: '{line}'");
    };
    Ok((String::from(key.trim()), String::from(value.trim())))
}

fn get_subtarget<T>(
    rule: &str,
    outputs: &mut Vec<PathBuf>,
    variables: &mut HashMap<String, String>,
) -> Result<Vec<T>, String>
where
    T: NinjaTarget,
{
    let mut targets = Vec::new();
    if !variables.contains_key("COMMAND") {
        return Ok(targets);
    }

    let mut subtarget_vars = variables.clone();
    *subtarget_vars.get_mut("COMMAND").unwrap() = String::from("cp $(in) $(out)");
    let cmd = variables.get_mut("COMMAND").unwrap();
    for output in outputs.iter_mut() {
        let renamed = Path::new("n2s").join(&*output);
        *cmd = cmd.replace(&path_to_string(&*output), &path_to_string(&renamed));
        let old_output = std::mem::replace(output, renamed.clone());
        targets.push(T::new(NinjaTargetCommon {
            rule: String::from(rule),
            outputs: vec![old_output],
            implicit_outputs: Vec::new(),
            inputs: vec![renamed],
            implicit_deps: Vec::new(),
            order_only_deps: Vec::new(),
            variables: subtarget_vars.clone(),
        }));
    }

    Ok(targets)
}

fn parse_build_target<T>(line: &str, lines: &mut Peekable<str::Lines>) -> Result<Vec<T>, String>
where
    T: NinjaTarget,
{
    let Some(line_stripped) = line.strip_prefix("build ") else {
        return error!("parse_build_target failed: '{line}'");
    };

    let (output_section, input_section) = split_output_and_input_sections(line_stripped)?;

    let (mut outputs, implicit_outputs) = parse_output_section(output_section)?;
    let (rule, inputs, implicit_deps, order_only_deps) = parse_input_section(input_section)?;

    let mut variables: HashMap<String, String> = HashMap::new();
    while let Some(next_line) = lines.next_if(|line| line.starts_with(" ")) {
        let (key, value) = parse_key_value(next_line)?;
        variables.insert(key, value);
    }

    let mut targets = Vec::new();
    if outputs.len() > 1 {
        targets.extend(get_subtarget(&rule, &mut outputs, &mut variables)?)
    }
    targets.push(T::new(NinjaTargetCommon {
        rule,
        outputs,
        implicit_outputs,
        inputs,
        implicit_deps,
        order_only_deps,
        variables,
    }));
    Ok(targets)
}

fn parse_ninja_rule(
    line: &str,
    lines: &mut Peekable<str::Lines>,
) -> Result<(String, NinjaRuleCmd), String> {
    let Some(rule) = line.strip_prefix("rule ") else {
        return error!("parse_ninja_rule failed: '{line}'");
    };
    let mut command = None;
    let mut rspfile = None;
    let mut rspfile_content = None;
    while let Some(next_line) = lines.next_if(|line| line.starts_with(" ")) {
        let (key, value) = parse_key_value(next_line)?;
        match key.as_str() {
            "command" => command = Some(value),
            "rspfile" => rspfile = Some(value),
            "rspfile_content" => rspfile_content = Some(value),
            _ => (),
        }
    }
    let Some(command) = command else {
        return error!("parse_ninja_rule failed");
    };
    Ok((
        String::from(rule),
        NinjaRuleCmd {
            command,
            rsp_info: rspfile.zip(rspfile_content),
        },
    ))
}

fn parse_subninja_file<T>(line: &str, dir_path: &Path) -> Result<(Vec<T>, NinjaRulesMap), String>
where
    T: NinjaTarget,
{
    let Some((_, subninja_file)) = line.split_once(" ") else {
        return error!("parse_subninja_file failed: '{line}'");
    };
    parse_ninja_file(dir_path.join(subninja_file), dir_path)
}

fn parse_ninja_file<T>(
    file_path: PathBuf,
    build_path: &Path,
) -> Result<(Vec<T>, NinjaRulesMap), String>
where
    T: NinjaTarget,
{
    let mut all_targets = Vec::new();
    let mut targets: Vec<T> = Vec::new();
    let mut globals = HashMap::new();
    let mut all_rules = HashMap::new();

    let file = read_file(&file_path)?.replace("$\n", " ");

    let mut lines = file.lines().peekable();
    while let Some(line) = lines.next() {
        if line.is_empty()
            || line.starts_with("default ")
            || line.starts_with("pool ")
            || line.starts_with("#")
            || line.starts_with(" ")
        {
            continue;
        } else if line.starts_with("rule ") {
            let (rule, rule_command) = parse_ninja_rule(line, &mut lines)?;
            all_rules.insert(rule, rule_command);
        } else if line.starts_with("build ") {
            targets.extend(parse_build_target(line, &mut lines)?);
        } else if line.starts_with("subninja ") || line.starts_with("include ") {
            let (subtargets, rules) = parse_subninja_file(line, build_path)?;
            all_targets.extend(subtargets);
            all_rules.extend(rules);
        } else {
            let (key, value) = parse_key_value(line)?;
            globals.insert(key, value);
        }
    }
    for target in &mut targets {
        target.set_globals(globals.clone());
    }
    all_targets.extend(targets);

    Ok((all_targets, all_rules))
}

pub fn parse_build_ninja<T>(build_path: &Path) -> Result<Vec<T>, String>
where
    T: NinjaTarget,
{
    let file_path = build_path.join("build.ninja");
    let (mut targets, rules) = parse_ninja_file::<T>(file_path, build_path)?;
    for target in &mut targets {
        target.set_rule(&rules);
    }
    Ok(targets)
}
