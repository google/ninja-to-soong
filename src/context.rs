// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::env;

use crate::project::*;
use crate::utils::*;

#[derive(Default, Clone)]
pub struct Context {
    pub projects_to_generate: VecDeque<ProjectId>,
    temp_path: PathBuf,
    clean_gen_ninja: bool,
    n2s_path: PathBuf,
    android_path: Option<PathBuf>,
    external_project_path: Option<PathBuf>,
    pub unittest_path: Option<PathBuf>,
    pub exe_path: PathBuf,
    pub skip_gen_ninja: bool,
    pub skip_build: bool,
    pub copy_to_aosp: bool,
    pub wildcardize_paths: bool,
}

const AOSP_PATH: &str = "--aosp-path";
const AOSP_PATH_SHORT: &str = "-p";
const EXT_PROJ_PATH: &str = "--ext-proj-path";
const EXT_PROJ_PATH_SHORT: &str = "-e";
const CLEAN_TMP: &str = "--clean-tmp";
const CLEAN_TMP_SHORT: &str = "-C";
const CLEAN_GEN_NINJA: &str = "--clean-gen-ninja";
const CLEAN_GEN_NINJA_SHORT: &str = "-c";
const COPY_TO_AOSP: &str = "--copy-to-aosp";
const COPY_TO_AOSP_SHORT: &str = "-a";
const SKIP_BUILD: &str = "--skip-build";
const SKIP_BUILD_SHORT: &str = "-s";
const SKIP_GEN_NINJA: &str = "--skip-gen-ninja";
const SKIP_GEN_NINJA_SHORT: &str = "-S";
const HELP: &str = "--help";
const HELP_SHORT: &str = "-h";

const TESTS_FOLDER: &str = "tests";
const SCRIPTS_FOLDER: &str = "scripts";
const TARGET_FOLDER: &str = "target";

impl Context {
    pub fn get_temp_path(&self, build_path: &Path) -> Result<PathBuf, String> {
        let path = self.temp_path.join(build_path);
        if self.clean_gen_ninja && path != self.temp_path && remove_dir(&path)? {
            print_info!("{0:#?} removed", path);
        }
        Ok(path)
    }

    pub fn get_test_path(&self, project: &dyn Project) -> PathBuf {
        match &self.unittest_path {
            Some(unittest_path) => unittest_path.clone(),
            None => self.n2s_path.join(TESTS_FOLDER).join(project.get_name()),
        }
    }

    pub fn get_script_path(&self, project: &dyn Project) -> PathBuf {
        self.n2s_path.join(SCRIPTS_FOLDER).join(project.get_name())
    }

    pub fn get_android_path(&self, project: &dyn Project) -> Result<PathBuf, String> {
        match &self.android_path {
            Some(android_path) => Ok(android_path.clone().join(project.get_android_path()?)),
            None => error!("'{AOSP_PATH}' has not been defined"),
        }
    }

    pub fn get_external_project_path(&self) -> Result<PathBuf, String> {
        match &self.external_project_path {
            Some(external_project_path) => Ok(external_project_path.clone()),
            None => error!("'{EXT_PROJ_PATH}' has not been defined"),
        }
    }

    fn get_partial_matching_project(projects_map: &ProjectsMap, target: &str) -> Option<ProjectId> {
        for (project_id, project) in projects_map.iter() {
            if project
                .get_name()
                .to_lowercase()
                .contains(&target.to_lowercase())
            {
                print_info!("Could not find perfect match for {target:#?}, but found partial match with {:#?}", project.get_name());
                return Some(*project_id);
            }
        }
        None
    }

    pub fn parse_args(projects_map: &ProjectsMap) -> Result<Self, String> {
        let args = env::args().collect::<Vec<String>>();
        let exec = file_name(&Path::new(&args[0]));
        let mut iter = args[1..].iter();
        let mut clean_tmp = false;
        let project_name_to_id = projects_map.iter().fold(
            std::collections::HashMap::new(),
            |mut map, (project_id, project)| {
                map.insert(project.get_name(), *project_id);
                map
            },
        );
        let mut ctx = Self::default();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                AOSP_PATH_SHORT | AOSP_PATH => {
                    let Some(path) = iter.next() else {
                        return error!("<path> missing for {AOSP_PATH}");
                    };
                    let android_path = PathBuf::from(path);
                    if !android_path.exists() {
                        return error!("{android_path:#?} does not exists");
                    }
                    ctx.android_path = Some(android_path);
                }
                EXT_PROJ_PATH_SHORT | EXT_PROJ_PATH => {
                    let Some(path) = iter.next() else {
                        return error!("<path> missing for {EXT_PROJ_PATH}");
                    };
                    let path = PathBuf::from(path);
                    if !path.exists() {
                        return error!("{path:#?} does not exists");
                    }
                    ctx.projects_to_generate.push_back(ProjectId::External);
                    ctx.external_project_path = Some(path);
                }
                SKIP_GEN_NINJA_SHORT | SKIP_GEN_NINJA => {
                    ctx.skip_gen_ninja = true;
                    ctx.skip_build = true
                }
                SKIP_BUILD_SHORT | SKIP_BUILD => ctx.skip_build = true,
                COPY_TO_AOSP_SHORT | COPY_TO_AOSP => {
                    ctx.copy_to_aosp = true;
                    ctx.wildcardize_paths = true;
                }
                CLEAN_GEN_NINJA_SHORT | CLEAN_GEN_NINJA => ctx.clean_gen_ninja = true,
                CLEAN_TMP_SHORT | CLEAN_TMP => clean_tmp = true,
                HELP_SHORT | HELP => {
                    let mut projects_help = projects_map
                        .iter()
                        .map(|(_, project)| format!("  {0}\n", project.get_name()))
                        .collect::<Vec<String>>();
                    projects_help.sort_unstable();
                    return error!(
                        "
USAGE: {exec} [OPTIONS] [PROJECTS]

PROJECTS:
{0}
OPTIONS:
{AOSP_PATH_SHORT}, {AOSP_PATH} <path>\t\tPath to Android tree
{EXT_PROJ_PATH_SHORT}, {EXT_PROJ_PATH} <path>\tPath to external project rust file
{CLEAN_TMP_SHORT}, {CLEAN_TMP}\t\t\tRemove temporary directory before running
{CLEAN_GEN_NINJA_SHORT}, {CLEAN_GEN_NINJA}\t\tRemove selected projects old build directories before running
{COPY_TO_AOSP_SHORT}, {COPY_TO_AOSP}\t\tCopy generated Soong files into the Android tree
{SKIP_BUILD_SHORT}, {SKIP_BUILD}\t\tSkip build step
{SKIP_GEN_NINJA_SHORT}, {SKIP_GEN_NINJA}\t\tSkip generation of Ninja files
{HELP_SHORT}, {HELP}\t\t\tDisplay the help and exit
",
                        projects_help.concat()
                    );
                }
                project => match project_name_to_id.get(project) {
                    Some(project) => ctx.projects_to_generate.push_back(*project),
                    None => match Self::get_partial_matching_project(projects_map, project) {
                        Some(project) => ctx.projects_to_generate.push_back(project),
                        None => return error!("Unknown project '{project}'"),
                    },
                },
            }
        }
        if (clean_tmp || ctx.clean_gen_ninja) && ctx.skip_gen_ninja {
            return error!("Clean & skip Ninja generation are incompatible");
        }
        // TEMP_PATH
        ctx.temp_path = if let Ok(dir) = env::var("N2S_TMP_PATH") {
            PathBuf::from(dir)
        } else {
            env::temp_dir()
        }
        .join(&exec);
        if clean_tmp && remove_dir(&ctx.temp_path)? {
            print_info!("{0:#?} removed", ctx.temp_path);
        }
        if create_dir(&ctx.temp_path)? {
            print_info!("{0:#?} created", ctx.temp_path);
        }
        // TEST_PATH
        match env::current_exe() {
            Ok(exe_path) => {
                ctx.exe_path = PathBuf::from(exe_path.parent().unwrap());
                let target = ctx
                    .exe_path // <ninja-to-soong>/target/<build-mode>
                    .parent() // <ninja-to-soong>/target
                    .unwrap();
                if file_name(target) != TARGET_FOLDER {
                    return error!("Executable is not in its expected build folder. Cannot figure out where test folder is.");
                }
                ctx.n2s_path = PathBuf::from(target.parent().unwrap());
                let root_contents = ls_dir(&ctx.n2s_path)?;
                for entry in [TESTS_FOLDER, SCRIPTS_FOLDER, TARGET_FOLDER] {
                    if !root_contents.contains(&ctx.n2s_path.join(entry)) {
                        return error!("Executable is not in its expected build folder. Cannot figure out where test folder is.");
                    }
                }
            }
            Err(err) => return error!("Could not get current executable path: {err}"),
        };
        if ctx.android_path.is_none() && ctx.n2s_path.ends_with("external/rust/ninja-to-soong") {
            ctx.android_path = Some(PathBuf::from(
                ctx.n2s_path // <aosp>/external/rust/ninja-to-soong
                    .parent() // <aosp>/external/rust
                    .unwrap()
                    .parent() // <aosp>/external
                    .unwrap()
                    .parent() // <aosp>
                    .unwrap(),
            ));
        }

        Ok(ctx)
    }
}
