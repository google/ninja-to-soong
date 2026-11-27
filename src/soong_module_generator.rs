// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use crate::context::*;
use crate::ninja_target::*;
use crate::project::*;
use crate::soong_module::*;
use crate::utils::*;

#[derive(Default)]
pub struct SoongModuleGeneratorInternals {
    pub gen_assets: Vec<PathBuf>,
    pub libs: Vec<PathBuf>,
    pub custom_cmd_inputs: Vec<PathBuf>,
    pub tools_module: Vec<PathBuf>,
    python_binaries: std::collections::HashSet<String>,
    python_libraries: std::collections::HashSet<String>,
}

struct ObjectModuleInfo {
    target_name: PathBuf,
    external_shared_libs: std::collections::BTreeSet<String>,
    internal_static_deps: Vec<String>,
}

struct PreparedCompileObject<'a> {
    input: &'a PathBuf,
    target: &'a NinjaTarget,
    defines: Vec<String>,
    cflags: Vec<String>,
    defines_map: std::collections::BTreeMap<String, String>,
}

struct DefineGroup<'a> {
    sig: Vec<Option<String>>,
    objects: Vec<PreparedCompileObject<'a>>,
}

pub struct SoongModuleGenerator<'a> {
    internals: SoongModuleGeneratorInternals,
    object_modules: std::collections::HashMap<String, ObjectModuleInfo>,
    bindgen_modules: std::collections::BTreeMap<String, String>,
    visited_includes: std::collections::HashSet<PathBuf>,
    src_path: &'a Path,
    ndk_path: &'a Path,
    build_path: &'a Path,
    gen_build_prefix: Option<&'a str>,
    targets_map: &'a NinjaTargetsMap<'a>,
    targets_to_gen: &'a NinjaTargetsToGenMap,
    project: &'a dyn Project,
}

impl<'a> SoongModuleGenerator<'a> {
    pub fn new(
        src_path: &'a Path,
        ndk_path: &'a Path,
        build_path: &'a Path,
        gen_build_prefix: Option<&'a str>,
        targets_map: &'a NinjaTargetsMap<'a>,
        targets_to_gen: &'a NinjaTargetsToGenMap,
        project: &'a dyn Project,
    ) -> Self {
        Self {
            internals: SoongModuleGeneratorInternals::default(),
            object_modules: std::collections::HashMap::new(),
            bindgen_modules: std::collections::BTreeMap::new(),
            visited_includes: std::collections::HashSet::new(),
            src_path,
            ndk_path,
            build_path,
            gen_build_prefix,
            targets_map,
            targets_to_gen,
            project,
        }
    }
    pub fn delete(self) -> SoongModuleGeneratorInternals {
        self.internals
    }

    pub fn filter_target(&self, target: &NinjaTarget) -> bool {
        let target_name = target.get_name();
        debug_project!("filter_target({target_name:#?})");
        self.project.filter_target(&target_name)
    }

    fn replace_path(&self, iter: impl Iterator<Item = String>) -> Vec<String> {
        let iter = iter.map(|path| {
            path.replace(&path_to_string_with_separator(self.src_path), "")
                .replace(&path_to_string(self.src_path), "")
        });
        if let Some(prefix) = self.gen_build_prefix {
            iter.map(|path| path.replace(&path_to_string(self.build_path), prefix))
                .collect()
        } else {
            iter.collect()
        }
    }
    fn get_sources(&mut self, sources: Vec<PathBuf>) -> Vec<String> {
        let mut filtered_sources = Vec::new();
        for source in sources {
            debug_project!("filter_source({source:#?})");
            if !self.project.filter_source(&source) {
                continue;
            }
            if let Ok(strip) = source.strip_prefix(self.build_path) {
                if self.targets_map.get(strip).is_none() && source.exists() {
                    self.internals.gen_assets.push(PathBuf::from(strip));
                }
            }
            filtered_sources.push(path_to_string(&source));
        }
        self.replace_path(filtered_sources.into_iter())
    }
    fn is_soong_default_define(define: &str) -> bool {
        let name = define.split_once('=').map_or(define, |(name, _)| name);
        matches!(name, "ANDROID" | "NDEBUG" | "_FORTIFY_SOURCE")
    }
    fn get_defines(&self, defines: Vec<String>) -> Vec<String> {
        self.replace_path(defines.into_iter().filter(|def| {
            if Self::is_soong_default_define(def) {
                return false;
            }
            debug_project!("filter_define({def})");
            self.project.filter_define(def)
        }))
        .iter()
        .map(|def| format!("-D{def}"))
        .collect()
    }
    fn get_cflags(&self, cflags: Vec<String>) -> Vec<String> {
        cflags
            .into_iter()
            .filter(|cflag| {
                debug_project!("filter_cflags({cflag})");
                self.project.filter_cflag(cflag)
            })
            .collect()
    }
    fn get_cfgs(
        &mut self,
        cfgs: Vec<String>,
        sources: &mut Vec<String>,
    ) -> Result<Vec<String>, String> {
        let android_path = path_to_string(self.project.get_android_path()?);
        let src_path = path_to_string(self.src_path);
        let mut filtered_cfgs = Vec::new();
        for cfg in cfgs {
            debug_project!("filter_cfg({cfg})");
            if !self.project.filter_cfg(&cfg) {
                continue;
            }
            if let Some((_, val)) = cfg.split_once("=\\\"") {
                if let Some(path) = val.strip_suffix("\\\"") {
                    if path.starts_with(&src_path) {
                        sources.extend(self.get_sources(vec![PathBuf::from(path)]));
                    }
                }
            }
            filtered_cfgs.push(cfg.replace(&src_path, &android_path));
        }
        Ok(filtered_cfgs)
    }
    fn collect_build_include_assets(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if file_name(&path) != "CMakeFiles" {
                    self.collect_build_include_assets(&path);
                }
            } else if ["h", "hpp", "inc", "def"].contains(&file_ext(&path).as_str()) {
                let strip = strip_prefix(&path, self.build_path);
                if self.targets_map.get(&strip).is_none() {
                    debug_project!("filter_gen_header({strip:#?})");
                    if !self.project.filter_gen_header(&strip) {
                        self.internals.gen_assets.push(strip);
                    }
                }
            }
        }
    }
    fn get_includes(&mut self, includes: Vec<PathBuf>) -> Vec<String> {
        let mut filtered_includes = Vec::new();
        for include in includes {
            debug_project!("filter_include({include:#?})");
            if !self.project.filter_include(&include) {
                continue;
            }
            if include.starts_with(self.build_path) && self.visited_includes.insert(include.clone())
            {
                self.collect_build_include_assets(&include);
            }
            filtered_includes.push(path_to_string(&include));
        }
        self.replace_path(filtered_includes.into_iter())
    }
    fn get_libs(
        &mut self,
        libs: Vec<PathBuf>,
        module_name: &String,
        kind: LibraryKind,
    ) -> Vec<(String, LibraryKind, bool)> {
        libs.into_iter()
            .filter_map(|lib| {
                debug_project!("filter_lib({lib:#?})");
                if !self.project.filter_lib(&path_to_string(&lib)) {
                    return None;
                }
                let is_internal = !lib.starts_with(&self.ndk_path)
                    && self
                        .targets_map
                        .get(&lib)
                        .is_some_and(|target| self.filter_target(target));
                Some(if lib.starts_with(&self.ndk_path) {
                    (file_stem(&lib), kind, is_internal)
                } else {
                    let (lib_path, lib_kind) = match self.project.map_lib(&lib, kind) {
                        Some((map_lib, lib_kind)) => (
                            self.targets_to_gen.get_name(&map_lib).unwrap_or(map_lib),
                            lib_kind,
                        ),
                        None => (
                            self.targets_to_gen
                                .get_name(&lib)
                                .unwrap_or_else(|| self.get_module_prefix().join(&lib)),
                            kind,
                        ),
                    };
                    let lib_id = path_to_id(lib_path);
                    if lib_id == *module_name {
                        return None;
                    }
                    self.internals.libs.push(lib);
                    (lib_id, lib_kind, is_internal)
                })
            })
            .collect()
    }
    fn get_link_flags(&self, link_flags: Vec<String>) -> Vec<String> {
        link_flags
            .into_iter()
            .filter(|flag| {
                debug_project!("filter_link_flag({flag})");
                self.project.filter_link_flag(flag)
            })
            .collect()
    }
    fn get_generated_assets_from(
        &mut self,
        roots: Vec<PathBuf>,
        filter_header: bool,
    ) -> Result<Vec<String>, String> {
        let mut gen_assets = Vec::new();
        self.targets_map
            .traverse_from(roots, !filter_header, |target| {
                if target.get_outputs().len() > 1
                    && !matches!(target.get_rule()?, NinjaRule::RustBindgen)
                {
                    return Ok(false);
                }
                if let NinjaRule::CustomCommand(_) = target.get_rule()? {
                    gen_assets.extend(target.get_outputs().clone())
                }
                Ok(true)
            })?;
        Ok(gen_assets
            .iter()
            .filter_map(|asset| {
                if file_ext(asset).starts_with(if filter_header { "c" } else { "h" }) {
                    return None;
                }
                debug_project!(
                    "filter_gen_{0}({asset:#?})",
                    if filter_header { "header" } else { "source" }
                );
                if (filter_header && !self.project.filter_gen_header(asset))
                    || (!filter_header && !self.project.filter_gen_source(asset))
                {
                    self.internals.gen_assets.push(PathBuf::from(asset));
                    return None;
                }
                let Some(target) = self.targets_map.get(asset) else {
                    self.internals.gen_assets.push(PathBuf::from(asset));
                    return None;
                };
                Some(match self.targets_to_gen.get_name(asset) {
                    Some(name) => path_to_string(name),
                    None => path_to_id(self.get_module_prefix().join(target.get_name())),
                })
            })
            .collect())
    }
    fn get_generated_sources(&mut self, target: &NinjaTarget) -> Result<Vec<String>, String> {
        self.get_generated_assets_from(target.get_outputs().clone(), false)
    }
    fn parse_define(flag: &str) -> Option<(&str, &str)> {
        let def = flag.strip_prefix("-D")?;
        Some(def.split_once('=').unwrap_or((def, "1")))
    }
    fn extend_defines_map(
        map: &mut std::collections::BTreeMap<String, String>,
        defines: &[String],
    ) {
        for flag in defines {
            if let Some((var, val)) = Self::parse_define(flag) {
                map.insert(String::from(var), String::from(val));
            }
        }
    }
    fn collect_defines_map(defines: &[String]) -> std::collections::BTreeMap<String, String> {
        let mut map = std::collections::BTreeMap::new();
        Self::extend_defines_map(&mut map, defines);
        map
    }
    fn sanitize_id_fragment(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut prev_sep = false;
        for c in s.chars() {
            if c.is_ascii_alphanumeric() || c == '-' {
                out.push(c);
                prev_sep = false;
            } else if c == '_' {
                out.push('_');
                prev_sep = true;
            } else if !prev_sep && !out.is_empty() {
                out.push('_');
                prev_sep = true;
            }
        }
        while out.ends_with('_') && !s.ends_with('_') {
            out.pop();
        }
        out
    }
    fn fnv1a_64(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
    fn hash_define_signature(conflicting_keys: &[String], sig: &[Option<String>]) -> String {
        let mut bytes = Vec::new();
        for (key, val) in conflicting_keys.iter().zip(sig.iter()) {
            if let Some(val) = val {
                bytes.extend_from_slice(key.as_bytes());
                bytes.push(b'=');
                bytes.extend_from_slice(val.as_bytes());
                bytes.push(0);
            }
        }
        let h = Self::fnv1a_64(&bytes);
        format!("{:08x}", ((h ^ (h >> 32)) & 0xffff_ffff) as u32)
    }
    fn format_define_group_suffix(conflicting_keys: &[String], sig: &[Option<String>]) -> String {
        const MAX_SUFFIX_LEN: usize = 64;
        let parts: Vec<String> = conflicting_keys
            .iter()
            .zip(sig.iter())
            .filter_map(|(key, val)| {
                let val = val.as_deref()?;
                let key_frag = Self::sanitize_id_fragment(key);
                let val_frag = Self::sanitize_id_fragment(val);
                Some(if val_frag.is_empty() {
                    key_frag
                } else {
                    format!("{key_frag}_{val_frag}")
                })
            })
            .collect();
        let suffix = if parts.is_empty() {
            String::from("default")
        } else {
            parts.join("_")
        };
        if suffix.len() > MAX_SUFFIX_LEN {
            let truncated = suffix[..MAX_SUFFIX_LEN].trim_end_matches('_');
            let hash = Self::hash_define_signature(conflicting_keys, sig);
            if truncated.is_empty() {
                hash
            } else {
                format!("{truncated}_{hash}")
            }
        } else {
            suffix
        }
    }
    fn common_parent_dir(paths: &[&PathBuf]) -> Option<PathBuf> {
        let first = paths.first()?.parent()?;
        let mut prefix: Vec<_> = first.components().collect();
        for path in &paths[1..] {
            let Some(parent) = path.parent() else {
                return None;
            };
            let common_len = prefix
                .iter()
                .zip(parent.components())
                .take_while(|(a, b)| **a == *b)
                .count();
            prefix.truncate(common_len);
            if prefix.is_empty() {
                return None;
            }
        }
        if prefix.is_empty() {
            None
        } else {
            Some(prefix.into_iter().collect())
        }
    }
    fn collect_target_libs(
        &mut self,
        target: &NinjaTarget,
        module_name: &String,
        libs: &mut Vec<(String, LibraryKind, bool)>,
    ) {
        libs.extend(self.get_libs(
            target.get_libs_static_whole(),
            module_name,
            LibraryKind::StaticWhole,
        ));
        libs.extend(self.get_libs(target.get_libs_static(), module_name, LibraryKind::Static));
        libs.extend(self.get_libs(target.get_libs_shared(), module_name, LibraryKind::Shared));
    }
    fn collect_target_sources_and_includes(
        &mut self,
        input_target: &NinjaTarget,
        module_name: &String,
        sources: &mut Vec<String>,
        includes: &mut Vec<String>,
        internal_static_deps: &mut Vec<String>,
    ) -> Result<(), String> {
        if let Some(bindgen_target) = input_target
            .get_inputs()
            .first()
            .and_then(|src| self.targets_map.get(src))
            .filter(|t| matches!(t.get_rule(), Ok(NinjaRule::RustBindgen)))
        {
            let bindgen_module_name = self.get_dep_id(&bindgen_target.get_name());
            sources.push(format!(":{bindgen_module_name}"));
            internal_static_deps.push(bindgen_module_name.clone());
            self.bindgen_modules
                .insert(bindgen_module_name, module_name.clone());
        } else {
            sources.extend(self.get_sources(input_target.get_sources(self.build_path)?));
        }
        includes.extend(self.get_includes(input_target.get_includes(self.build_path)));
        Ok(())
    }
    fn partition_compile_objects<'b>(
        compile_objects: Vec<PreparedCompileObject<'b>>,
        parent_defines_map: &std::collections::BTreeMap<String, String>,
        has_parent_sources: bool,
    ) -> (
        Vec<String>,
        Vec<PreparedCompileObject<'b>>,
        Vec<DefineGroup<'b>>,
    ) {
        let mut define_values: std::collections::BTreeMap<&str, std::collections::HashSet<&str>> =
            std::collections::BTreeMap::new();
        for (k, v) in parent_defines_map {
            define_values
                .entry(k.as_str())
                .or_default()
                .insert(v.as_str());
        }
        for obj in &compile_objects {
            for (k, v) in &obj.defines_map {
                define_values
                    .entry(k.as_str())
                    .or_default()
                    .insert(v.as_str());
            }
        }
        let conflicting_keys: Vec<String> = define_values
            .into_iter()
            .filter(|(_, vals)| vals.len() > 1)
            .map(|(k, _)| String::from(k))
            .collect();
        if conflicting_keys.is_empty() {
            return (conflicting_keys, compile_objects, Vec::new());
        }

        let mut groups: Vec<DefineGroup<'b>> = Vec::new();
        let mut group_indices: std::collections::HashMap<Vec<Option<String>>, usize> =
            std::collections::HashMap::new();
        for obj in compile_objects {
            let sig: Vec<Option<String>> = conflicting_keys
                .iter()
                .map(|k| obj.defines_map.get(k).cloned())
                .collect();
            if let Some(&idx) = group_indices.get(&sig) {
                groups[idx].objects.push(obj);
            } else {
                let idx = groups.len();
                group_indices.insert(sig.clone(), idx);
                groups.push(DefineGroup {
                    sig,
                    objects: vec![obj],
                });
            }
        }

        let parent_sig: Vec<Option<String>> = conflicting_keys
            .iter()
            .map(|k| parent_defines_map.get(k).cloned())
            .collect();
        let parent_group_idx = group_indices.get(&parent_sig).copied().or_else(|| {
            if !has_parent_sources && parent_sig.iter().all(Option::is_none) {
                groups
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, g)| g.sig.iter().filter(|v| v.is_some()).count())
                    .map(|(idx, _)| idx)
            } else {
                None
            }
        });
        let parent_objects = match parent_group_idx {
            Some(idx) => groups.remove(idx).objects,
            None => Vec::new(),
        };
        (conflicting_keys, parent_objects, groups)
    }
    fn generate_split_object_group(
        &mut self,
        split_module_name: String,
        split_target_name: PathBuf,
        objects: Vec<PreparedCompileObject>,
        ctx: &Context,
    ) -> Result<SoongModule, String> {
        let mut cflags = Vec::new();
        let mut includes = Vec::new();
        let mut sources = Vec::new();
        let mut internal_static_deps = Vec::new();
        let mut roots = Vec::with_capacity(objects.len());

        for obj in objects {
            roots.push(obj.input.clone());
            self.collect_target_sources_and_includes(
                obj.target,
                &split_module_name,
                &mut sources,
                &mut includes,
                &mut internal_static_deps,
            )?;
            cflags.extend(obj.defines);
            cflags.extend(obj.cflags);
        }

        let generated_headers = self.get_generated_assets_from(roots.clone(), true)?;
        let generated_sources = self.get_generated_assets_from(roots, false)?;

        let mut srcs_prop = SoongNamedProp::new("srcs", sources);
        if ctx.wildcardize_paths {
            srcs_prop.enable_wildcard(&self.src_path)?;
        }
        let module = SoongModule::new("cc_library_static")
            .add_prop("name", split_module_name.clone())
            .add_named_prop(srcs_prop)
            .add_prop("cflags", cflags)
            .add_prop("shared_libs", Vec::<String>::new())
            .add_prop("local_include_dirs", includes)
            .add_prop("generated_sources", generated_sources)
            .add_prop("generated_headers", generated_headers);

        self.object_modules.insert(
            split_module_name,
            ObjectModuleInfo {
                target_name: split_target_name,
                external_shared_libs: std::collections::BTreeSet::new(),
                internal_static_deps,
            },
        );

        Ok(module)
    }
    pub fn generate_object(
        &mut self,
        module_type: &str,
        target: &NinjaTarget,
        ctx: &Context,
    ) -> Result<Vec<SoongModule>, String> {
        let target_name = target.get_name();
        let module_name = path_to_id(
            self.targets_to_gen
                .get_name(&target_name)
                .unwrap_or_else(|| self.get_module_prefix().join(&target_name)),
        );
        let mut modules = Vec::new();
        let mut cflags = Vec::new();
        let mut includes = Vec::new();
        let mut sources = Vec::new();
        let mut libs = Vec::new();
        let mut whole_static_libs = Vec::new();
        let mut internal_static_deps = Vec::new();

        let target_defines = self.get_defines(target.get_defines());
        let target_cflags = self.get_cflags(target.get_cflags());
        let mut parent_defines_map = std::collections::BTreeMap::new();
        let mut parent_source_roots = Vec::new();
        let mut has_parent_sources = false;

        let mut compile_objects = Vec::new();
        for input in target.get_inputs() {
            let Some(input_target) = self.targets_map.get(input) else {
                sources.push(path_to_string(strip_prefix(
                    canonicalize_path(input, self.build_path),
                    self.src_path,
                )));
                continue;
            };
            if input_target.is_compile_object() {
                let defines = self.get_defines(input_target.get_defines());
                let obj_cflags = self.get_cflags(input_target.get_cflags());
                let defines_map = Self::collect_defines_map(&defines);
                compile_objects.push(PreparedCompileObject {
                    input,
                    target: input_target,
                    defines,
                    cflags: obj_cflags,
                    defines_map,
                });
            } else {
                parent_source_roots.push(input.clone());
                if input_target.is_custom_command() && file_ext(input).starts_with('c') {
                    has_parent_sources = true;
                }
                self.collect_target_libs(input_target, &module_name, &mut libs);
                self.collect_target_sources_and_includes(
                    input_target,
                    &module_name,
                    &mut sources,
                    &mut includes,
                    &mut internal_static_deps,
                )?;
                let input_defines = self.get_defines(input_target.get_defines());
                Self::extend_defines_map(&mut parent_defines_map, &input_defines);
                cflags.extend(input_defines);
                cflags.extend(self.get_cflags(input_target.get_cflags()));
            }
        }
        Self::extend_defines_map(&mut parent_defines_map, &target_defines);
        has_parent_sources |= !sources.is_empty();

        let (conflicting_keys, parent_objects, split_groups) = Self::partition_compile_objects(
            compile_objects,
            &parent_defines_map,
            has_parent_sources,
        );

        if !split_groups.is_empty() {
            let base_suffixes: Vec<String> = split_groups
                .iter()
                .map(|g| Self::format_define_group_suffix(&conflicting_keys, &g.sig))
                .collect();
            let mut suffix_counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for suffix in &base_suffixes {
                *suffix_counts.entry(suffix.as_str()).or_insert(0) += 1;
            }
            for (group, base_suffix) in split_groups.into_iter().zip(&base_suffixes) {
                let final_suffix = if suffix_counts[base_suffix.as_str()] > 1 {
                    format!(
                        "{base_suffix}_{}",
                        Self::hash_define_signature(&conflicting_keys, &group.sig)
                    )
                } else {
                    base_suffix.clone()
                };
                let split_module_name = format!("{module_name}_{final_suffix}");
                let group_input_paths: Vec<&PathBuf> =
                    group.objects.iter().map(|obj| obj.input).collect();
                let split_target_name = Self::common_parent_dir(&group_input_paths)
                    .filter(|dir| dir != &target_name)
                    .unwrap_or_else(|| {
                        PathBuf::from(format!("{}_{final_suffix}", path_to_string(&target_name)))
                    });
                let split_module = self.generate_split_object_group(
                    split_module_name.clone(),
                    split_target_name,
                    group.objects,
                    ctx,
                )?;
                modules.push(split_module);
                internal_static_deps.push(split_module_name.clone());
                whole_static_libs.push(split_module_name);
            }
        }

        for obj in parent_objects {
            parent_source_roots.push(obj.input.clone());
            self.collect_target_sources_and_includes(
                obj.target,
                &module_name,
                &mut sources,
                &mut includes,
                &mut internal_static_deps,
            )?;
            cflags.extend(obj.defines);
            cflags.extend(obj.cflags);
        }
        includes.extend(self.get_includes(target.get_includes(self.build_path)));
        cflags.extend(target_defines);
        cflags.extend(target_cflags);

        let mut parent_header_roots = parent_source_roots.clone();
        parent_header_roots.extend(target.get_implicit_deps().iter().cloned());
        parent_header_roots.extend(target.get_order_only_deps().iter().cloned());
        let generated_headers = self.get_generated_assets_from(parent_header_roots, true)?;
        let generated_sources = self.get_generated_assets_from(parent_source_roots, false)?;
        let (version_script, link_flags) = target.get_link_flags();
        let link_flags = self.get_link_flags(link_flags);
        self.collect_target_libs(target, &module_name, &mut libs);
        let mut static_libs = Vec::new();
        let mut shared_libs = Vec::new();
        let mut external_shared_libs = std::collections::BTreeSet::new();
        for (lib, kind, is_internal) in libs {
            match kind {
                LibraryKind::StaticWhole => {
                    if is_internal {
                        internal_static_deps.push(lib.clone());
                    }
                    whole_static_libs.push(lib);
                }
                LibraryKind::Static => {
                    if is_internal {
                        internal_static_deps.push(lib.clone());
                    }
                    static_libs.push(lib);
                }
                LibraryKind::Shared => {
                    if is_internal {
                        shared_libs.push(lib);
                    } else {
                        external_shared_libs.insert(lib);
                    }
                }
                LibraryKind::Unspecified => (),
            }
        }

        let module_type = self
            .targets_to_gen
            .get_module_name(&target_name)
            .unwrap_or_else(|| String::from(module_type));
        let mut module = SoongModule::new(&module_type).add_prop("name", module_name.clone());
        if let Some(stem) = self.targets_to_gen.get_stem(&target_name) {
            module = module.add_prop("stem", stem);
        }
        if let Some(vs) = version_script {
            module = module.add_prop(
                "version_script",
                path_to_string(strip_prefix(vs, &self.src_path)),
            );
        }
        let mut srcs_prop = SoongNamedProp::new("srcs", sources);
        if ctx.wildcardize_paths {
            srcs_prop.enable_wildcard(&self.src_path)?;
        }
        module = module
            .add_named_prop(srcs_prop)
            .add_prop("cflags", cflags)
            .add_prop("ldflags", link_flags)
            .add_prop("shared_libs", shared_libs)
            .add_prop("static_libs", static_libs)
            .add_prop("whole_static_libs", whole_static_libs)
            .add_prop("local_include_dirs", includes)
            .add_prop("generated_sources", generated_sources)
            .add_prop("generated_headers", generated_headers);

        self.object_modules.insert(
            module_name,
            ObjectModuleInfo {
                target_name,
                external_shared_libs,
                internal_static_deps,
            },
        );

        modules.push(module);
        Ok(modules)
    }

    pub fn generate_rust(
        &mut self,
        module_type: &str,
        target: &NinjaTarget,
    ) -> Result<Vec<SoongModule>, String> {
        if target.get_inputs().len() != 1 {
            return error!("Invalid inputs in: {target:#?}");
        }
        let target_name = target.get_name();
        let module_name = path_to_id(
            self.targets_to_gen
                .get_name(&target_name)
                .unwrap_or_else(|| self.get_module_prefix().join(&target_name)),
        );
        let stem = self
            .targets_to_gen
            .get_stem(&target_name)
            .unwrap_or_else(|| file_stem(&target_name));
        let mut modules = Vec::new();
        let mut sources = Vec::new();
        let input = &target.get_inputs()[0];
        let crate_root = match self.targets_map.get(input) {
            Some(input_target) if matches!(input_target.get_rule()?, NinjaRule::None) => {
                let genrule_name =
                    path_to_id(self.get_module_prefix().join(input.parent().unwrap()));
                let mut copy_srcs = Vec::new();
                let mut copy_outs = Vec::new();
                for dep in target.get_order_only_deps() {
                    if file_ext(dep) != "rs" {
                        continue;
                    }
                    let Some(dep_target) = self.targets_map.get(dep) else {
                        continue;
                    };
                    if let Some(bindgen_target) = dep_target
                        .get_inputs()
                        .first()
                        .and_then(|src| self.targets_map.get(src))
                        .filter(|t| matches!(t.get_rule(), Ok(NinjaRule::RustBindgen)))
                    {
                        copy_srcs.push(format!(":{}", self.get_dep_id(&bindgen_target.get_name())));
                    } else {
                        copy_srcs
                            .extend(self.get_sources(dep_target.get_sources(self.build_path)?));
                    }
                    copy_outs.push(file_name(dep));
                }
                modules.push(
                    SoongModule::new("cc_genrule")
                        .add_prop("name", genrule_name.clone())
                        .add_prop("soc_specific", true)
                        .add_prop("cmd", "cp $(in) $(genDir)")
                        .add_prop("srcs", copy_srcs)
                        .add_prop("out", copy_outs),
                );
                sources.push(format!(":{genrule_name}"));
                Some(format!(":{genrule_name}{{{0}}}", file_name(input)))
            }
            Some(input_target) if matches!(input_target.get_rule()?, NinjaRule::RustBindgen) => {
                sources.push(format!(":{}", self.get_dep_id(&input_target.get_name())));
                None
            }
            _ => Some(
                self.get_sources(vec![canonicalize_path(input, self.build_path)])
                    .pop()
                    .unwrap_or_default(),
            ),
        };
        let rustlibs = self
            .get_libs(
                target.get_rustlibs(),
                &module_name,
                LibraryKind::Unspecified,
            )
            .into_iter()
            .map(|(lib, _, _)| lib)
            .collect::<Vec<_>>();
        let proc_macros = self
            .get_libs(
                target.get_proc_macros(),
                &module_name,
                LibraryKind::Unspecified,
            )
            .into_iter()
            .map(|(lib, _, _)| lib)
            .collect::<Vec<_>>();
        let cfgs = self.get_cfgs(target.get_cfgs(), &mut sources)?;
        self.get_generated_sources(target)?;
        let mut module = SoongModule::new(module_type)
            .add_prop("name", module_name)
            .add_prop("stem", stem)
            .add_prop("crate_name", target.get_crate_name()?);
        if let Some(crate_root) = crate_root {
            module = module.add_prop("crate_root", crate_root);
        }
        let module = module
            .add_prop("srcs", sources)
            .add_prop("edition", target.get_edition()?)
            .add_prop("cfgs", cfgs)
            .add_prop("rustlibs", rustlibs)
            .add_prop("proc_macros", proc_macros);
        modules.push(self.project.extend_rust(&target_name, module)?);
        Ok(modules)
    }

    pub fn generate_rust_bindgen(
        &mut self,
        target: &NinjaTarget,
    ) -> Result<Vec<SoongModule>, String> {
        let target_name = target.get_name();
        let module_name = self.get_dep_id(&target_name);
        let source_stem = file_stem(&target_name);
        let stem = format!("lib{source_stem}");
        let wrapper_src = self
            .get_sources(target.get_sources(self.build_path)?)
            .pop()
            .unwrap_or_default();
        let mut cflags = self.get_defines(target.get_defines());
        cflags.extend(self.get_cflags(target.get_cflags()));
        let includes = self.get_includes(target.get_includes(self.build_path));
        let mut module = SoongModule::new("rust_bindgen")
            .add_prop("name", module_name.clone())
            .add_prop("stem", stem)
            .add_prop("crate_name", source_stem.clone())
            .add_prop("wrapper_src", wrapper_src)
            .add_prop("source_stem", source_stem)
            .add_prop("bindgen_flags", target.get_bindgen_flags())
            .add_prop("handle_static_inline", true);
        if let Some(c_std) = target.get_c_std() {
            module = module.add_prop("c_std", c_std);
        }
        module = module
            .add_prop("cflags", cflags)
            .add_prop("local_include_dirs", includes)
            .add_prop("shared_libs", Vec::<String>::new())
            .add_prop(
                "rlib",
                SoongProp::Prop(vec![SoongNamedProp::new("enabled", false)]),
            )
            .add_prop(
                "dylib",
                SoongProp::Prop(vec![SoongNamedProp::new("enabled", false)]),
            );

        self.object_modules.insert(
            module_name,
            ObjectModuleInfo {
                target_name,
                external_shared_libs: std::collections::BTreeSet::new(),
                internal_static_deps: Vec::new(),
            },
        );

        Ok(vec![module])
    }

    pub fn finalize_objects(&mut self, modules: &mut Vec<SoongModule>) -> Result<(), String> {
        loop {
            let mut updates = Vec::new();
            for info in self.object_modules.values() {
                for dep in &info.internal_static_deps {
                    updates.push((dep.clone(), info.external_shared_libs.clone()));
                }
            }
            let mut changed = false;
            for (dep, shared_libs) in updates {
                let Some(dep_info) = self.object_modules.get_mut(&dep) else {
                    continue;
                };
                for lib in shared_libs {
                    changed |= dep_info.external_shared_libs.insert(lib);
                }
            }
            if !changed {
                break;
            }
        }
        for module in &mut *modules {
            let Some(SoongProp::Str(module_name)) =
                module.get_prop("name").map(|prop| prop.get_prop())
            else {
                continue;
            };
            let Some(info) = self.object_modules.get(&module_name) else {
                continue;
            };
            let m = std::mem::replace(module, SoongModule::new("")).extend_prop(
                "shared_libs",
                info.external_shared_libs
                    .iter()
                    .map(|s| s.as_str())
                    .collect(),
            )?;
            *module = self.project.extend_module(&info.target_name, m)?;
        }
        let mut header_modules = Vec::new();
        for (bindgen_name, inline_lib_name) in &self.bindgen_modules {
            let mut gen_headers = Vec::new();
            if let Some(inline_mod) = modules.iter_mut().find(|m| {
                m.get_prop("name").map(|p| p.get_prop())
                    == Some(SoongProp::Str(inline_lib_name.clone()))
            }) {
                if let Some(SoongProp::VecStr(v)) = inline_mod
                    .get_prop("generated_headers")
                    .map(|p| p.get_prop())
                {
                    gen_headers = v;
                }
                if let Some(info) = self.object_modules.get(inline_lib_name) {
                    let m = std::mem::replace(inline_mod, SoongModule::new(""))
                        .add_prop("stem", file_stem(&info.target_name));
                    *inline_mod = m;
                }
            }
            let headers_name = format!("{bindgen_name}_headers");
            header_modules.push(
                SoongModule::new("cc_library_headers")
                    .add_prop("name", headers_name.clone())
                    .add_prop("soc_specific", true)
                    .add_prop("generated_headers", gen_headers.clone())
                    .add_prop("export_generated_headers", gen_headers),
            );
            if let Some(bindgen_mod) = modules.iter_mut().find(|m| {
                m.get_prop("name").map(|p| p.get_prop())
                    == Some(SoongProp::Str(bindgen_name.clone()))
            }) {
                let m = std::mem::replace(bindgen_mod, SoongModule::new(""))
                    .add_prop("static_inline_library", inline_lib_name.clone())
                    .add_prop("header_libs", vec![headers_name]);
                *bindgen_mod = m;
            }
        }
        modules.extend(header_modules);
        Ok(())
    }

    fn map_cmd_output(&self, output: &Path) -> String {
        if output.starts_with("n2s") {
            path_to_string(output)
        } else if let Some(output) = self.project.map_cmd_output(output) {
            output
        } else {
            path_to_string(output)
        }
    }
    fn replace_in_splitted_cmd(
        &self,
        cmd: &str,
        inputs: &Vec<PathBuf>,
        outputs: &Vec<PathBuf>,
        deps: &Vec<(PathBuf, String)>,
        depfile: Option<&Path>,
    ) -> String {
        let cmd = String::from(cmd);
        if cmd.is_empty() {
            return cmd;
        }
        for input in inputs {
            let canonicalize_input = path_to_string(canonicalize_path(input, self.build_path));
            let input_string = path_to_string(&input);
            let froms = if input_string.contains(&canonicalize_input) {
                vec![input_string, canonicalize_input]
            } else {
                vec![canonicalize_input, input_string]
            };
            if froms.contains(&cmd) {
                return format!(
                    "$(location {0})",
                    path_to_string(strip_prefix(
                        canonicalize_path(input, self.build_path),
                        self.src_path,
                    ))
                );
            }
        }
        for (dep, dep_target_name) in deps {
            if vec![
                path_to_string(canonicalize_path(&dep, self.build_path)),
                path_to_string(&dep),
            ]
            .contains(&cmd)
            {
                return format!("$(location :{dep_target_name})");
            }
        }
        for output in outputs {
            if vec![
                path_to_string(canonicalize_path(output, self.build_path)),
                path_to_string(output),
                file_name(output),
            ]
            .contains(&cmd)
            {
                return format!(
                    "$(location {0})",
                    path_to_string(self.map_cmd_output(output))
                );
            }
        }
        if let Some(depfile) = depfile {
            if [
                path_to_string(canonicalize_path(depfile, self.build_path)),
                path_to_string(depfile),
            ]
            .contains(&cmd)
            {
                if let Some(output) = outputs.first() {
                    return format!(
                        "$$(dirname $(location {0}))/{1}",
                        path_to_string(self.map_cmd_output(output)),
                        file_name(depfile),
                    );
                }
            }
        }
        for input in inputs {
            let canonicalize_input = canonicalize_path(input, self.build_path);
            for from in vec![
                path_to_string(canonicalize_input.parent().unwrap()),
                path_to_string(input.parent().unwrap()),
            ] {
                if let Some(suffix) = cmd.strip_prefix(&from) {
                    return format!(
                        "$$(dirname $(location {0})){1}",
                        path_to_string(strip_prefix(&canonicalize_input, self.src_path,)),
                        suffix,
                    );
                }
            }
        }
        let src_path_str = path_to_string(self.src_path);
        if !src_path_str.is_empty() {
            if let Some(suffix) = cmd.strip_prefix(&src_path_str) {
                if suffix.is_empty() || suffix.starts_with('/') {
                    if let Some(input) = inputs
                        .iter()
                        .map(|i| canonicalize_path(i, self.build_path))
                        .find(|i| i.starts_with(self.src_path))
                    {
                        let rel_input = strip_prefix(&input, self.src_path);
                        let up_count = rel_input.parent().map_or(0, |p| p.components().count());
                        let up_dirs = "/..".repeat(up_count);
                        return format!(
                            "$$(dirname $(location {0})){up_dirs}{suffix}",
                            path_to_string(&rel_input),
                        );
                    }
                }
            }
        }
        for output in outputs {
            let output_string = path_to_string(output.parent().unwrap());
            let canonicalize_output = canonicalize_path(&output_string, self.build_path);
            let stripped_output = strip_prefix(&canonicalize_output, self.build_path.join("n2s"));
            let stripped_string = path_to_string(&stripped_output);
            let mut froms = vec![
                path_to_string(&canonicalize_output),
                path_to_string(canonicalize_path(&stripped_output, self.build_path)),
            ];
            if !output_string.is_empty() {
                froms.push(output_string);
            }
            if !stripped_string.is_empty() {
                froms.push(stripped_string);
            }
            if froms.contains(&cmd) {
                return format!(
                    "$$(dirname $(location {0}))",
                    path_to_string(self.map_cmd_output(output)),
                );
            }
            if let Some(dir) = cmd.strip_suffix('/') {
                if froms.iter().any(|from| from == dir) {
                    return format!(
                        "$$(dirname $(location {0}))/",
                        path_to_string(self.map_cmd_output(output)),
                    );
                }
            }
        }
        cmd
    }
    fn split_cmd(
        &self,
        mut separators: Vec<&str>,
        cmd: &str,
        inputs: &Vec<PathBuf>,
        outputs: &Vec<PathBuf>,
        deps: &Vec<(PathBuf, String)>,
        depfile: Option<&Path>,
    ) -> String {
        let Some(separator) = separators.pop() else {
            return self.replace_in_splitted_cmd(cmd, inputs, outputs, deps, depfile);
        };
        cmd.split(separator)
            .into_iter()
            .map(|split| self.split_cmd(separators.clone(), split, inputs, outputs, deps, depfile))
            .collect::<Vec<_>>()
            .join(separator)
    }
    fn get_cmd(
        &self,
        target_name: &Path,
        mut cmd: String,
        rule_cmd: NinjaRuleCmd,
        inputs: Vec<PathBuf>,
        outputs: &Vec<PathBuf>,
        deps: Vec<(PathBuf, String)>,
    ) -> String {
        cmd = self.split_cmd(
            vec![" ", "=", "-I", ",", ":"],
            &cmd,
            &inputs,
            outputs,
            &deps,
            rule_cmd.depfile.as_deref(),
        );
        if deps.is_empty() && inputs.len() > 1 {
            let input_paths = inputs
                .iter()
                .map(|file| {
                    path_to_string(strip_prefix(
                        canonicalize_path(file, self.build_path),
                        self.src_path,
                    ))
                })
                .collect::<Vec<_>>();
            let mut sorted_paths = input_paths.clone();
            sorted_paths.sort_unstable();
            sorted_paths.dedup();
            if input_paths == sorted_paths || self.project.allow_unordered_cmd_inputs(target_name) {
                let inputs_locations = input_paths
                    .iter()
                    .map(|file_path| format!("$(location {file_path})"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if cmd.contains(&inputs_locations) {
                    cmd = cmd.replace(&inputs_locations, "$(in)");
                }
            }
        }
        if let Some((rsp_file, rsp_content)) = rule_cmd.rsp_info {
            let rsp_inputs = rsp_content
                .split(" ")
                .filter_map(|file| {
                    if file.is_empty() {
                        return None;
                    }
                    Some(PathBuf::from(file))
                })
                .collect::<Vec<PathBuf>>();
            let rsp_inputs_string = if inputs == rsp_inputs {
                String::from("$(in)")
            } else {
                rsp_inputs
                    .iter()
                    .map(|file| {
                        let file_path = path_to_string(strip_prefix(
                            canonicalize_path(file, self.build_path),
                            self.src_path,
                        ));
                        format!("$(location {file_path})")
                    })
                    .collect::<Vec<String>>()
                    .join(" ")
            };
            let rsp = format!("$(genDir)/{rsp_file}");
            cmd = format!("echo \\\"{rsp_inputs_string}\\\" > {rsp} && {cmd}")
                .replace("${rspfile}", &rsp);
        }
        cmd
    }
    fn get_module_prefix(&self) -> PathBuf {
        self.project
            .map_module_prefix()
            .unwrap_or_else(|| PathBuf::from(self.project.get_name()))
    }
    fn get_dep_id(&self, input: &Path) -> String {
        let Some(target) = self.targets_map.get(&input) else {
            return path_to_id(self.get_module_prefix().join(&input));
        };
        return if target.get_outputs().len() > 1
            && !matches!(target.get_rule(), Ok(NinjaRule::RustBindgen))
        {
            path_to_id(self.get_module_prefix().join(target.get_name()))
                + "{"
                + &path_to_string(&input)
                + "}"
        } else {
            path_to_id(self.get_module_prefix().join(target.get_name()))
        };
    }
    fn get_cmd_inputs(
        &self,
        inputs: Vec<PathBuf>,
        deps: &mut Vec<(PathBuf, String)>,
    ) -> Vec<PathBuf> {
        inputs
            .into_iter()
            .filter_map(|input| {
                if let Some(mapped_input) = self.project.map_cmd_input(&input) {
                    deps.push((input, mapped_input));
                    return None;
                }
                if canonicalize_path(&input, self.build_path).starts_with(self.build_path) {
                    deps.push((input.clone(), self.get_dep_id(&input)));
                    return None;
                }
                Some(input)
            })
            .collect()
    }
    fn get_tool_module(
        &mut self,
        tool: &Path,
        python_inputs: Vec<PathBuf>,
    ) -> Result<Option<(String, Option<Vec<SoongModule>>)>, String> {
        if let Some(tool_module) = self.project.map_tool_module(tool) {
            self.internals.tools_module.push(tool_module.clone());
            return Ok(Some((path_to_id(tool_module), None)));
        } else if let Ok(tool_target_path) = Path::new(tool).strip_prefix(self.build_path) {
            return Ok(Some((
                String::from(":") + &self.get_dep_id(tool_target_path),
                None,
            )));
        } else if !file_name(tool).ends_with(".py") {
            return Ok(None);
        }
        let tool_module = path_to_id(self.get_module_prefix().join(tool));
        if !self.internals.python_binaries.contains(&tool_module) {
            let mut modules = Vec::new();
            let (src, main) = if file_stem(tool).contains(".") {
                let new_tool = path_to_id(PathBuf::from(tool)) + ".py";
                let name = path_to_id(self.get_module_prefix().join(&new_tool));
                modules.push(
                    SoongModule::new("genrule")
                        .add_prop("name", name.clone())
                        .add_prop("cmd", "cp $(in) $(out)")
                        .add_prop("srcs", vec![path_to_string(tool)])
                        .add_prop("out", vec![new_tool.clone()]),
                );
                (String::from(":") + &name, new_tool)
            } else {
                (path_to_string(tool), path_to_string(tool))
            };

            let mut srcs = Vec::new();
            for python_input in python_inputs {
                let name = path_to_id(self.get_module_prefix().join(&python_input).join("cp"));
                let lib_full_name = path_to_string(&python_input);
                if !self.internals.python_libraries.contains(&lib_full_name) {
                    modules.push(
                        SoongModule::new("genrule")
                            .add_prop("name", name.clone())
                            .add_prop("cmd", "cp $(in) $(out)")
                            .add_prop("srcs", vec![lib_full_name.clone()])
                            .add_prop("out", vec![file_name(&python_input)]),
                    );
                    self.internals.python_libraries.insert(lib_full_name);
                }
                srcs.push(String::from(":") + &name);
            }

            srcs.push(src);
            srcs.sort_unstable();
            srcs.dedup();
            let multiple_srcs = srcs.len() > 1;
            let module = SoongModule::new("python_binary_host")
                .add_prop("name", tool_module.clone())
                .add_prop("main", main)
                .add_prop("srcs", srcs);
            let extended_module = self
                .project
                .extend_python_binary_host(&self.src_path.join(&tool), module.clone())?;
            if !multiple_srcs && module == extended_module {
                return Ok(None);
            }
            self.internals.python_binaries.insert(tool_module.clone());
            modules.push(extended_module);
            return Ok(Some((tool_module, Some(modules))));
        }
        Ok(Some((tool_module, None)))
    }
    fn escape_genrule_cmd(cmd: &str) -> String {
        cmd.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "$$")
            .replace("$${rspfile}", "${rspfile}")
    }
    fn get_tools(
        &mut self,
        mut cmd: String,
        inputs: &mut Vec<PathBuf>,
    ) -> Result<(Vec<String>, Vec<String>, Vec<SoongModule>, String), String> {
        if cmd.starts_with("cp") || cmd.starts_with("echo ") {
            return Ok((Vec::new(), Vec::new(), Vec::new(), cmd));
        }
        while let Some(index) = cmd.find("python") {
            let begin = cmd[..index].rfind(" ").unwrap_or_default();
            cmd = match cmd[index..].find(" ") {
                Some(end) => cmd.replace(&cmd[begin..index + end + 1], ""),
                None => cmd.replace(&cmd[begin..], ""),
            };
        }
        let tool_location = String::from("$(location) ");
        let (tool, mut cmd) = if let Some((tool, cmd)) = cmd.split_once(" ") {
            (String::from(tool), tool_location + cmd)
        } else {
            (String::from(cmd), tool_location)
        };
        let mut tool_modules = Vec::new();
        let glslang_validator = String::from("glslangValidator");
        let tool_location = String::from("$(location ") + &tool + ")";
        if let Some(idx) = inputs
            .iter()
            .position(|input| path_to_string(input) == tool)
        {
            inputs.remove(idx);
        }
        if let Some(idx) = inputs
            .iter()
            .position(|input| input.ends_with(&glslang_validator))
        {
            let input = path_to_string(inputs.remove(idx));
            tool_modules.push(glslang_validator.clone());
            cmd = cmd.replace("$(location)", &tool_location).replace(
                &input,
                &(String::from("$(location ") + &glslang_validator + ")"),
            );
        }
        inputs.retain(|input| !file_name(input).starts_with("python"));
        let tool = strip_prefix(canonicalize_path(&tool, self.build_path), self.src_path);
        let python_inputs = inputs
            .iter()
            .filter_map(|input| {
                if path_to_string(input).ends_with(".py") {
                    Some(strip_prefix(
                        canonicalize_path(input, self.build_path),
                        self.src_path,
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if let Some((tool_module, some_modules)) = self.get_tool_module(&tool, python_inputs)? {
            cmd = cmd.replace(
                &tool_location,
                &(String::from("$(location ") + &tool_module + ")"),
            );
            tool_modules.push(tool_module);
            return Ok((
                Vec::new(),
                tool_modules,
                some_modules.unwrap_or_default(),
                cmd,
            ));
        }
        if path_to_string(&tool).ends_with(".py") {
            cmd = String::from("python3 ") + &cmd;
        }
        let tool_name = file_name(&tool);
        if ["bison", "flex"].contains(&tool_name.as_str()) {
            tool_modules.push(tool_name.clone());
            tool_modules.push(String::from("m4"));
            return Ok((
                Vec::new(),
                tool_modules,
                Vec::new(),
                String::from("M4=$(location m4) ")
                    + &cmd.replace(
                        "$(location)",
                        &(String::from("$(location ") + &tool_name + ")"),
                    ),
            ));
        }
        if tool_name == "glslangValidator" {
            tool_modules.push(glslang_validator.clone());
            return Ok((Vec::new(), tool_modules, Vec::new(), cmd));
        }
        Ok((vec![path_to_string(tool)], tool_modules, Vec::new(), cmd))
    }
    pub fn generate_custom_command(
        &mut self,
        target: &NinjaTarget,
        rule_cmd: NinjaRuleCmd,
    ) -> Result<Vec<SoongModule>, String> {
        let mut inputs = Vec::new();
        let mut deps = Vec::new();
        inputs.extend(self.get_cmd_inputs(target.get_inputs().clone(), &mut deps));
        inputs.extend(self.get_cmd_inputs(target.get_implicit_deps().clone(), &mut deps));
        let cmd = if rule_cmd.is_soong_cmd {
            rule_cmd.command.clone()
        } else {
            Self::escape_genrule_cmd(&rule_cmd.command)
        };
        let (tool_files, tool_modules, mut modules, cmd) = self.get_tools(cmd, &mut inputs)?;
        let mut sources = inputs
            .iter()
            .map(|input| {
                path_to_string(strip_prefix(
                    canonicalize_path(input, self.build_path),
                    self.src_path,
                ))
            })
            .collect::<Vec<String>>();
        for (dep, dep_target_name) in &deps {
            let src = format!(":{dep_target_name}");
            if !tool_modules.contains(&src) {
                sources.push(src);
            }
            if let Some(dep_target) = self.targets_map.get(dep) {
                if !self.filter_target(dep_target) {
                    self.internals.custom_cmd_inputs.push(dep.clone());
                }
            } else {
                self.internals.custom_cmd_inputs.push(dep.clone());
            }
        }
        let target_outputs = target.get_outputs();
        let target_name = target.get_name();
        let cmd = self.get_cmd(&target_name, cmd, rule_cmd, inputs, target_outputs, deps);
        let outputs = target_outputs
            .iter()
            .map(|output| path_to_string(self.map_cmd_output(output)))
            .collect::<Vec<_>>();
        let module_name = match self.targets_to_gen.get_name(&target_name) {
            Some(name) => path_to_string(name),
            None => path_to_id(self.get_module_prefix().join(target_name)),
        };

        modules.push(
            self.project.extend_custom_command(
                &target.get_name(),
                SoongModule::new("cc_genrule")
                    .add_prop("name", module_name)
                    .add_prop("cmd", cmd)
                    .add_prop("srcs", sources)
                    .add_prop("out", outputs)
                    .add_prop("tools", tool_modules)
                    .add_prop("tool_files", tool_files),
            )?,
        );
        Ok(modules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soong_package::SoongPackage;

    struct MockProject {
        filter_gen_source_val: bool,
        filter_gen_header_val: bool,
    }

    impl MockProject {
        fn new(filter_gen_source_val: bool, filter_gen_header_val: bool) -> Self {
            Self {
                filter_gen_source_val,
                filter_gen_header_val,
            }
        }
    }

    impl Project for MockProject {
        fn get_name(&self) -> &'static str {
            "mock"
        }
        fn get_android_path(&self) -> Result<PathBuf, String> {
            Ok(PathBuf::from("external/mock"))
        }
        fn generate_package(
            &mut self,
            _ctx: &Context,
            _projects_map: &ProjectsMap,
        ) -> Result<String, String> {
            Ok(String::new())
        }
        fn filter_gen_source(&self, _source: &Path) -> bool {
            self.filter_gen_source_val
        }
        fn filter_gen_header(&self, _header: &Path) -> bool {
            self.filter_gen_header_val
        }
        fn filter_target(&self, target: &Path) -> bool {
            self.filter_gen_source_val || !["c", "h"].contains(&file_ext(target).as_str())
        }
    }

    fn make_target(
        output: &str,
        inputs: &[&str],
        order_only: &[&str],
        command: &str,
    ) -> NinjaTarget {
        NinjaTarget::new(
            NinjaTargetCommon {
                rule: String::from("test_rule"),
                outputs: vec![PathBuf::from(output)],
                implicit_outputs: Vec::new(),
                inputs: inputs.iter().map(PathBuf::from).collect(),
                implicit_deps: Vec::new(),
                order_only_deps: order_only.iter().map(PathBuf::from).collect(),
            },
            Some(ResolvedRuleCommand {
                command: String::from(command),
                rsp_info: None,
                depfile: None,
                description: None,
                is_generator: false,
            }),
        )
    }

    fn get_vec_prop(module: &SoongModule, prop_name: &str) -> Vec<String> {
        let Some(prop) = module.get_prop(prop_name) else {
            return Vec::new();
        };
        let SoongProp::VecStr(mut values) = prop.get_prop() else {
            panic!("expected VecStr for {prop_name}");
        };
        values.sort_unstable();
        values.dedup();
        values
    }

    #[test]
    fn default_soong_defines_are_filtered_by_name() {
        for def in [
            "ANDROID",
            "ANDROID=1",
            "NDEBUG",
            "NDEBUG=1",
            "_FORTIFY_SOURCE",
            "_FORTIFY_SOURCE=2",
            "_FORTIFY_SOURCE=3",
        ] {
            assert!(
                SoongModuleGenerator::is_soong_default_define(def),
                "expected default define: {def}"
            );
        }
        for def in [
            "ANDROID_API_LEVEL=35",
            "ANDROID_STRICT",
            "NDEBUG_EXTRA",
            "FOO=ANDROID",
            "FORTIFY_SOURCE=2",
        ] {
            assert!(
                !SoongModuleGenerator::is_soong_default_define(def),
                "unexpected default define: {def}"
            );
        }
    }

    #[test]
    fn escape_genrule_cmd_escapes_special_chars_and_preserves_rspfile() {
        assert_eq!(
            SoongModuleGenerator::escape_genrule_cmd("gen.py ${rspfile} out.h"),
            "gen.py ${rspfile} out.h"
        );
        assert_eq!(
            SoongModuleGenerator::escape_genrule_cmd(
                r#"echo $FOO && sed 's/foo$/bar/' '-DFOO="a b"' -DBAR=\(1\)"#
            ),
            r#"echo $$FOO && sed 's/foo$$/bar/' '-DFOO=\"a b\"' -DBAR=\\(1\\)"#
        );
    }

    #[test]
    fn get_cmd_uses_in_only_when_inputs_are_sorted_or_allowed() {
        struct TestProject {
            allow_unordered: bool,
        }
        impl Project for TestProject {
            fn get_name(&self) -> &'static str {
                "test"
            }
            fn get_android_path(&self) -> Result<PathBuf, String> {
                Ok(PathBuf::from("external/test"))
            }
            fn generate_package(
                &mut self,
                _ctx: &Context,
                _projects_map: &ProjectsMap,
            ) -> Result<String, String> {
                Ok(String::new())
            }
            fn allow_unordered_cmd_inputs(&self, target: &Path) -> bool {
                self.allow_unordered && target == Path::new("out.gz")
            }
        }

        let make_rule_cmd = |cmd: &str| -> NinjaRuleCmd {
            let target = NinjaTarget::new(
                NinjaTargetCommon {
                    rule: String::from("gen"),
                    outputs: vec![PathBuf::from("out.gz")],
                    implicit_outputs: Vec::new(),
                    inputs: Vec::new(),
                    implicit_deps: Vec::new(),
                    order_only_deps: Vec::new(),
                },
                Some(ResolvedRuleCommand {
                    command: String::from(cmd),
                    rsp_info: None,
                    depfile: None,
                    description: None,
                    is_generator: false,
                }),
            );
            let Ok(NinjaRule::CustomCommand(rule_cmd)) = target.get_rule() else {
                panic!("expected CustomCommand");
            };
            rule_cmd
        };

        let targets = Vec::new();
        let targets_map = NinjaTargetsMap::new(&targets);
        let targets_to_gen = NinjaTargetsToGenMap::from(&[]);
        let strict_proj = TestProject {
            allow_unordered: false,
        };
        let gen = SoongModuleGenerator::new(
            Path::new("/src"),
            Path::new("/ndk"),
            Path::new("/build"),
            None,
            &targets_map,
            &targets_to_gen,
            &strict_proj,
        );

        let sorted_cmd = gen.get_cmd(
            Path::new("out.gz"),
            String::from("python3 $(location) /src/a.xml /src/b.xml out.gz"),
            make_rule_cmd("python3 gen.py /src/a.xml /src/b.xml out.gz"),
            vec![PathBuf::from("/src/a.xml"), PathBuf::from("/src/b.xml")],
            &vec![PathBuf::from("out.gz")],
            Vec::new(),
        );
        assert_eq!(sorted_cmd, "python3 $(location) $(in) $(location out.gz)");

        let unsorted_cmd = gen.get_cmd(
            Path::new("out.gz"),
            String::from("python3 $(location) /src/b.xml /src/a.xml out.gz"),
            make_rule_cmd("python3 gen.py /src/b.xml /src/a.xml out.gz"),
            vec![PathBuf::from("/src/b.xml"), PathBuf::from("/src/a.xml")],
            &vec![PathBuf::from("out.gz")],
            Vec::new(),
        );
        assert_eq!(
            unsorted_cmd,
            "python3 $(location) $(location b.xml) $(location a.xml) $(location out.gz)"
        );

        let unordered_proj = TestProject {
            allow_unordered: true,
        };
        let gen_allowed = SoongModuleGenerator::new(
            Path::new("/src"),
            Path::new("/ndk"),
            Path::new("/build"),
            None,
            &targets_map,
            &targets_to_gen,
            &unordered_proj,
        );
        let allowed_cmd = gen_allowed.get_cmd(
            Path::new("out.gz"),
            String::from("python3 $(location) /src/b.xml /src/a.xml out.gz"),
            make_rule_cmd("python3 gen.py /src/b.xml /src/a.xml out.gz"),
            vec![PathBuf::from("/src/b.xml"), PathBuf::from("/src/a.xml")],
            &vec![PathBuf::from("out.gz")],
            Vec::new(),
        );
        assert_eq!(allowed_cmd, "python3 $(location) $(in) $(location out.gz)");
    }

    #[test]
    fn parse_define_handles_bare_empty_and_valued_defines() {
        assert_eq!(
            SoongModuleGenerator::parse_define("-DFOO=bar"),
            Some(("FOO", "bar"))
        );
        assert_eq!(
            SoongModuleGenerator::parse_define("-DFOO"),
            Some(("FOO", "1"))
        );
        assert_eq!(
            SoongModuleGenerator::parse_define("-DFOO="),
            Some(("FOO", ""))
        );
        assert_eq!(
            SoongModuleGenerator::parse_define(r#"-DG_LOG_DOMAIN=\"PluginA\""#),
            Some(("G_LOG_DOMAIN", r#"\"PluginA\""#))
        );
        assert_eq!(SoongModuleGenerator::parse_define("-Wall"), None);
    }

    #[test]
    fn format_define_group_suffix_sanitizes_and_bounds_length() {
        assert_eq!(
            SoongModuleGenerator::format_define_group_suffix(
                &[String::from("G_LOG_DOMAIN")],
                &[Some(String::from(r#"\"FuPluginAcpiDmar\""#))],
            ),
            "G_LOG_DOMAIN_FuPluginAcpiDmar"
        );
        assert_eq!(
            SoongModuleGenerator::format_define_group_suffix(
                &[String::from("FOO"), String::from("BAR")],
                &[None, Some(String::from("42"))],
            ),
            "BAR_42"
        );
        assert_eq!(
            SoongModuleGenerator::format_define_group_suffix(&[String::from("FOO")], &[None],),
            "default"
        );
        assert_eq!(
            SoongModuleGenerator::format_define_group_suffix(
                &[String::from("FOO")],
                &[Some(String::new())],
            ),
            "FOO"
        );
        let long_val = "A".repeat(100);
        let suffix = SoongModuleGenerator::format_define_group_suffix(
            &[String::from("LONG_KEY")],
            &[Some(long_val)],
        );
        assert!(suffix.starts_with("LONG_KEY_AAA"));
        // 64 truncated chars + "_" + 8 hex hash chars
        assert_eq!(suffix.len(), 64 + 1 + 8);
    }

    #[test]
    fn split_group_preserves_generated_sources_when_filter_gen_source_is_false() {
        let src_path = Path::new("/src");
        let build_path = Path::new("/build");
        let targets = vec![
            make_target(
                "plugins/a/a_struct.c",
                &["/src/plugins/a/a.rs"],
                &[],
                "python3 gen.py /src/plugins/a/a.rs plugins/a/a_struct.c",
            ),
            make_target(
                "plugins/b/b_struct.c",
                &["/src/plugins/b/b.rs"],
                &[],
                "python3 gen.py /src/plugins/b/b.rs plugins/b/b_struct.c",
            ),
            make_target(
                "plugins/a/a.c.o",
                &["/src/plugins/a/a.c"],
                &[],
                r#"clang -c '-DG_LOG_DOMAIN="A"' -o plugins/a/a.c.o /src/plugins/a/a.c"#,
            ),
            make_target(
                "plugins/a/a_struct.c.o",
                &["plugins/a/a_struct.c"],
                &[],
                r#"clang -c '-DG_LOG_DOMAIN="A"' -o plugins/a/a_struct.c.o plugins/a/a_struct.c"#,
            ),
            make_target(
                "plugins/b/b_struct.c.o",
                &["plugins/b/b_struct.c"],
                &[],
                r#"clang -c '-DG_LOG_DOMAIN="B"' -o plugins/b/b_struct.c.o plugins/b/b_struct.c"#,
            ),
            make_target(
                "src/core.c.o",
                &["/src/src/core.c"],
                &[],
                "clang -c -o src/core.c.o /src/src/core.c",
            ),
            make_target(
                "src/libengine.a",
                &[
                    "plugins/a/a.c.o",
                    "plugins/a/a_struct.c.o",
                    "plugins/b/b_struct.c.o",
                    "src/core.c.o",
                ],
                &[],
                "llvm-ar rcs src/libengine.a plugins/a/a.c.o plugins/a/a_struct.c.o plugins/b/b_struct.c.o src/core.c.o",
            ),
        ];
        let project = MockProject::new(false, false);
        let ctx = Context::default();
        let mut package = SoongPackage::new(&[], "mock_license", &[], &[])
            .generate_from_map(
                NinjaTargetsToGenMap::from(&[target!("src/libengine.a")]),
                NinjaTargetsMap::new(&targets),
                src_path,
                src_path,
                build_path,
                Some("meson_generated"),
                &project,
                &ctx,
            )
            .unwrap();
        let gen_deps = package.get_dep_gen_assets();
        package
            .filter_gen_deps("meson_generated", &gen_deps)
            .unwrap();

        assert_eq!(
            gen_deps,
            vec![
                PathBuf::from("plugins/a/a_struct.c"),
                PathBuf::from("plugins/b/b_struct.c"),
            ]
        );
        let parent = package.pop_module("mock_src_libengine_a").unwrap();
        assert_eq!(get_vec_prop(&parent, "srcs"), vec!["src/core.c"]);
        assert_eq!(get_vec_prop(&parent, "cflags"), Vec::<String>::new());
        assert_eq!(
            get_vec_prop(&parent, "generated_sources"),
            Vec::<String>::new()
        );
        assert_eq!(
            get_vec_prop(&parent, "whole_static_libs"),
            vec![
                "mock_src_libengine_a_G_LOG_DOMAIN_A",
                "mock_src_libengine_a_G_LOG_DOMAIN_B",
            ]
        );

        let group_a = package
            .pop_module("mock_src_libengine_a_G_LOG_DOMAIN_A")
            .unwrap();
        assert_eq!(
            get_vec_prop(&group_a, "srcs"),
            vec!["meson_generated/plugins/a/a_struct.c", "plugins/a/a.c"]
        );
        assert_eq!(
            get_vec_prop(&group_a, "cflags"),
            vec![r#"-DG_LOG_DOMAIN=\"A\""#]
        );

        let group_b = package
            .pop_module("mock_src_libengine_a_G_LOG_DOMAIN_B")
            .unwrap();
        assert_eq!(
            get_vec_prop(&group_b, "srcs"),
            vec!["meson_generated/plugins/b/b_struct.c"]
        );
    }

    #[test]
    fn bare_vs_zero_define_conflicts_and_sanitization_collisions_are_disambiguated() {
        let src_path = Path::new("/src");
        let build_path = Path::new("/build");
        let targets = vec![
            // Bare -DFLAG vs -DFLAG=1 are equivalent; -DFLAG=0 conflicts.
            make_target(
                "a.o",
                &["/src/a.c"],
                &[],
                r#"clang -c -DFLAG -DEXPR="a+b" -o a.o /src/a.c"#,
            ),
            make_target(
                "b.o",
                &["/src/b.c"],
                &[],
                r#"clang -c -DFLAG=1 -DEXPR="a+b" -o b.o /src/b.c"#,
            ),
            make_target(
                "c.o",
                &["/src/c.c"],
                &[],
                r#"clang -c -DFLAG=0 -DEXPR="a*b" -o c.o /src/c.c"#,
            ),
            make_target(
                "d.o",
                &["/src/d.c"],
                &[],
                r#"clang -c -DFLAG=0 -DEXPR="a/b" -o d.o /src/d.c"#,
            ),
            make_target(
                "libmixed.a",
                &["a.o", "b.o", "c.o", "d.o"],
                &[],
                "llvm-ar rcs libmixed.a a.o b.o c.o d.o",
            ),
        ];
        let project = MockProject::new(true, true);
        let ctx = Context::default();
        let mut package = SoongPackage::new(&[], "mock_license", &[], &[])
            .generate_from_map(
                NinjaTargetsToGenMap::from(&[target!("libmixed.a")]),
                NinjaTargetsMap::new(&targets),
                src_path,
                src_path,
                build_path,
                None,
                &project,
                &ctx,
            )
            .unwrap();

        // No unset group exists, so the first group (a.o + b.o) stays in parent.
        let parent = package.pop_module("mock_libmixed_a").unwrap();
        assert_eq!(get_vec_prop(&parent, "srcs"), vec!["a.c", "b.c"]);
        let whole_libs = get_vec_prop(&parent, "whole_static_libs");
        // c.o (EXPR="a*b") and d.o (EXPR="a/b") sanitize to the same base suffix,
        // so both must get distinct hash-disambiguated names.
        assert_eq!(whole_libs.len(), 2);
        assert_ne!(whole_libs[0], whole_libs[1]);
        for name in &whole_libs {
            assert!(
                name.starts_with("mock_libmixed_a_EXPR_a_b_FLAG_0_"),
                "unexpected split module name: {name}"
            );
            assert!(package.pop_module(name).is_some());
        }
    }
}
