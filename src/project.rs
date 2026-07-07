// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::context::*;
use crate::ninja_parser::*;
use crate::ninja_target::common::*;
use crate::ninja_target::*;
use crate::soong_module::*;
use crate::soong_package::*;
use crate::soong_package_merger::*;
use crate::utils::*;

pub mod common;
pub mod mesa3d_desktop;

define_ProjectId!(
    (Angle, angle),
    (Clpeak, clpeak),
    (Clvk, clvk),
    (Clspv, clspv),
    (Fwupd, fwupd),
    (LibCLC, libclc),
    (LlvmProject, llvm_project),
    (MediaDriver, media_driver),
    (Mesa3DDesktopIntel, mesa3d_desktop_intel),
    (Mesa3DDesktopPanVK, mesa3d_desktop_panvk),
    (OpenclCts, opencl_cts),
    (OpenclHeaders, opencl_headers),
    (OpenclIcdLoader, opencl_icd_loader),
    (SpirvHeaders, spirv_headers),
    (SpirvTools, spirv_tools),
    (UnitTest, unittest),
    (Vkoverhead, vkoverhead)
);
impl ProjectId {
    pub fn get_deps(&self) -> Vec<ProjectId> {
        let mut projects = std::collections::HashSet::new();
        for gen_deps in get_deps() {
            let (project_dep, projects_dep) = gen_deps.projects();
            if &project_dep == self {
                projects.extend(projects_dep);
            }
        }
        Vec::from_iter(projects)
    }
    pub fn get_android_path(self, map: &ProjectsMap, ctx: &Context) -> Result<PathBuf, String> {
        ctx.get_android_path(map.get(self)?.as_ref())
    }
    pub fn get_visibility(self, map: &ProjectsMap) -> Result<String, String> {
        Ok(String::from("//") + &path_to_string(map.get(self)?.as_ref().get_android_path()?))
    }
}

define_Dep!(
    (ClangHeaders, LlvmProject, (Clspv)),
    (ClspvTargets, Clspv, (Clvk)),
    (LibclcBins, LibCLC, (Clspv)),
    (LlvmProjectTargets, LlvmProject, (Clvk, LibCLC)),
    (SpirvHeaders, SpirvHeaders, (Clspv, SpirvTools, OpenclCts)),
    (SpirvToolsTargets, SpirvTools, (Clvk, OpenclCts))
);
impl Dep {
    pub fn get_id(self, input: &Path, prefix: &Path, build_path: &Path) -> String {
        path_to_id(Path::new(&format!("{self:#?}")).join(strip_prefix(
            canonicalize_path(input, build_path),
            canonicalize_path(prefix, build_path),
        )))
    }
    pub fn get_ninja_targets(
        self,
        projects_map: &ProjectsMap,
    ) -> Result<Vec<NinjaTargetToGen>, String> {
        let mut all_deps = Vec::new();
        for project in self.projects().1 {
            all_deps.extend(projects_map.get(project)?.get_deps(self));
        }
        Ok(all_deps)
    }
    pub fn get(self, projects_map: &ProjectsMap) -> Result<Vec<PathBuf>, String> {
        let mut all_deps = self
            .get_ninja_targets(projects_map)?
            .into_iter()
            .map(|target| PathBuf::from(target.path))
            .collect::<Vec<_>>();
        all_deps.sort_unstable();
        all_deps.dedup();
        Ok(all_deps)
    }
    pub fn get_visibilities(self, projects_map: &ProjectsMap) -> Result<Vec<String>, String> {
        let mut projects = Vec::new();
        for project in self.projects().1 {
            projects.push(project.get_visibility(projects_map)?);
        }
        Ok(projects)
    }
}

pub struct ProjectsMap(HashMap<ProjectId, Box<dyn Project>>);
impl ProjectsMap {
    pub fn new() -> Self {
        Self(get_projects())
    }
    pub fn insert(&mut self, id: ProjectId, project: Box<dyn Project>) {
        self.0.insert(id, project);
    }
    pub fn remove(&mut self, id: &ProjectId) -> Result<Box<dyn Project>, String> {
        let Some(entry) = self.0.remove(id) else {
            return error!("'{id:#?}' not found in projects map");
        };
        Ok(entry)
    }
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, ProjectId, Box<dyn Project>> {
        self.0.iter()
    }
    pub fn get(&self, id: ProjectId) -> Result<&Box<dyn Project>, String> {
        let Some(project) = self.0.get(&id) else {
            return error!("'{id:#?}' not found in projects map");
        };
        Ok(project)
    }
}

pub trait Project {
    // MANDATORY FUNCTIONS
    fn get_name(&self) -> &'static str;
    fn get_android_path(&self) -> Result<PathBuf, String>;
    fn generate_package(
        &mut self,
        ctx: &Context,
        projects_map: &ProjectsMap,
    ) -> Result<String, String>;
    // DEPENDENCIES FUNCTIONS
    fn get_deps(&self, _dep: Dep) -> Vec<NinjaTargetToGen> {
        Vec::new()
    }
    // EXTEND FUNCTIONS
    fn extend_module(&self, _target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        Ok(module)
    }
    fn extend_custom_command(
        &self,
        _target: &Path,
        module: SoongModule,
    ) -> Result<SoongModule, String> {
        Ok(module)
    }
    fn extend_python_binary_host(
        &self,
        _python_binary_path: &Path,
        module: SoongModule,
    ) -> Result<SoongModule, String> {
        Ok(module)
    }
    // MAP FUNCTIONS
    fn map_cmd_input(&self, _input: &Path) -> Option<String> {
        None
    }
    fn map_cmd_output(&self, _output: &Path) -> Option<String> {
        None
    }
    fn map_lib(&self, _lib: &Path, _kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        None
    }
    fn map_tool_module(&self, _tool_module: &Path) -> Option<PathBuf> {
        None
    }
    fn map_module_prefix(&self) -> Option<PathBuf> {
        None
    }
    // FILTER FUNCTIONS
    fn filter_cflag(&self, _cflag: &str) -> bool {
        true
    }
    fn filter_define(&self, _define: &str) -> bool {
        true
    }
    fn filter_gen_header(&self, _header: &Path) -> bool {
        true
    }
    fn filter_gen_source(&self, _source: &Path) -> bool {
        true
    }
    fn filter_include(&self, _include: &Path) -> bool {
        true
    }
    fn filter_lib(&self, _lib: &str) -> bool {
        true
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        true
    }
    fn filter_source(&self, _source: &Path) -> bool {
        true
    }
    fn filter_target(&self, _target: &Path) -> bool {
        true
    }
}
