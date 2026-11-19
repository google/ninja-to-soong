// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct UnitTest();

impl Project for UnitTest {
    fn get_name(&self) -> &'static str {
        "unittests"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(PathBuf::from(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        let test_path = ctx.get_test_path(self);
        print_verbose!("'{}'", file_name(&test_path));
        let config = read_file(&test_path.join("config"))?;
        let targets_to_gen = config
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|target| target!(target))
            .collect::<Vec<_>>();
        SoongPackage::new(&[], "unittest_license", &[], &[])
            .generate(
                NinjaTargetsToGenMap::from(&targets_to_gen),
                &test_path,
                &test_path,
                &test_path,
                None,
                self,
                ctx,
            )?
            .print(ctx)
    }
}
