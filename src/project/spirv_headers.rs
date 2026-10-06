// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct SpirvHeaders();

impl Project for SpirvHeaders {
    fn get_name(&self) -> &'static str {
        "SPIRV-Headers"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        let src_path = ctx.get_android_path(self)?;
        SoongPackage::new(
            &[],
            "SPIRV-Headers_license",
            &["SPDX-license-identifier-MIT"],
            &["LICENSE"],
        )
        .add_visibilities(Dep::SpirvHeaders.get_visibilities(projects_map)?)
        .add_visibilities(vec![ProjectId::Clvk.get_visibility(projects_map)?])
        .add_module(SoongModule::new_cc_library_headers(
            CcLibraryHeaders::SpirvHeaders,
            vec![String::from("include")],
        ))
        .add_module(SoongModule::new_cc_library_headers(
            CcLibraryHeaders::SpirvHeadersUnified1,
            vec![String::from("include/spirv/unified1")],
        ))
        .add_modules(Dep::SpirvHeaders.get_filegroup_modules(
            &src_path,
            Path::new(""),
            projects_map,
        )?)
        .print(ctx)
    }
}
