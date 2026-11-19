// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

extern crate ninja_to_soong;
use ninja_to_soong::context::*;
use ninja_to_soong::ninja_target::*;
use ninja_to_soong::project::*;
use ninja_to_soong::soong_package::*;
use ninja_to_soong::utils::*;

#[no_mangle]
pub fn get_project() -> Box<dyn Project> {
    Box::new(ExternalProject::default())
}

#[derive(Default)]
pub struct ExternalProject();

impl Project for ExternalProject {
    fn get_name(&self) -> &'static str {
        "external-project"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        error!("Not implemented")
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        let test_path = PathBuf::from(ctx.get_external_project_path()?.parent().unwrap());
        SoongPackage::new(
            &["//visibility:public"],
            "external-project_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[target!("external_project_library")]),
            &test_path,
            &PathBuf::from("ndk"),
            &test_path,
            None,
            self,
            ctx,
        )?
        .print(ctx)
    }
}
