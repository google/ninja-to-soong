// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::utils::*;

pub mod cmake;
pub mod common;
pub mod gn;
pub mod meson;

pub use cmake::*;
pub use gn::*;
pub use meson::*;

#[derive(Debug, Clone)]
pub struct NinjaRuleCmd {
    pub command: String,
    pub rsp_info: Option<(String, String)>,
}
pub type NinjaRulesMap = HashMap<String, NinjaRuleCmd>;

pub enum NinjaRule {
    Binary,
    StaticLibrary,
    SharedLibrary,
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
    pub variables: HashMap<String, String>,
}

pub trait NinjaTarget: std::fmt::Debug {
    // MANDATORY FUNCTIONS
    fn new(common: NinjaTargetCommon) -> Self;
    fn get_common(&self) -> &NinjaTargetCommon;
    fn get_rule(&self) -> Result<NinjaRule, String>;
    fn get_sources(&self, build_path: &Path) -> Result<Vec<PathBuf>, String>;
    fn get_libs_static(&self) -> Vec<PathBuf>;
    fn get_libs_static_whole(&self) -> Vec<PathBuf>;
    fn get_libs_shared(&self) -> Vec<PathBuf>;
    fn get_link_flags(&self) -> (Option<PathBuf>, Vec<String>);
    fn get_defines(&self) -> Vec<String>;
    fn get_includes(&self, build_path: &Path) -> Vec<PathBuf>;
    fn get_cflags(&self) -> Vec<String>;
    // OPTIONAL FUNCTIONS
    fn set_globals(&mut self, _globals: HashMap<String, String>) {}
    fn set_rule(&mut self, _rules: &NinjaRulesMap) {}
    // COMMON FUNCTIONS
    fn get_name(&self) -> PathBuf {
        PathBuf::from(&self.get_common().outputs[0])
    }
    fn get_inputs(&self) -> &Vec<PathBuf> {
        &self.get_common().inputs
    }
    fn get_implicit_deps(&self) -> &Vec<PathBuf> {
        &self.get_common().implicit_deps
    }
    fn get_order_only_deps(&self) -> &Vec<PathBuf> {
        &self.get_common().order_only_deps
    }
    fn get_outputs(&self) -> &Vec<PathBuf> {
        &self.get_common().outputs
    }
    fn get_implicit_outputs(&self) -> &Vec<PathBuf> {
        &self.get_common().implicit_outputs
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
pub struct NinjaTargetsMap<'a, T>(HashMap<&'a Path, &'a T>)
where
    T: NinjaTarget;

impl<'a, T> NinjaTargetsMap<'a, T>
where
    T: NinjaTarget,
{
    pub fn new(targets: &'a Vec<T>) -> Self {
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
    pub fn get(&self, key: &Path) -> Option<&&T> {
        self.0.get(key)
    }
    pub fn traverse_from<F>(
        &self,
        mut targets: Vec<PathBuf>,
        only_inputs: bool,
        mut filter_target: F,
    ) -> Result<(), String>
    where
        F: FnMut(&T) -> Result<bool, String>,
    {
        let mut targets_seen = std::collections::HashSet::new();
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
