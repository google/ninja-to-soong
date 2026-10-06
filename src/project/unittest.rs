// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct UnitTest();

fn generate_package<T>(
    targets: Vec<T>,
    targets_to_gen: &[NinjaTargetToGen],
    test_path: &Path,
    project: &UnitTest,
    ctx: &Context,
) -> Result<String, String>
where
    T: NinjaTarget,
{
    SoongPackage::new(&[], "unittest_license", &[], &[])
        .generate(
            NinjaTargetsToGenMap::from(targets_to_gen),
            targets,
            test_path,
            test_path,
            test_path,
            None,
            project,
            ctx,
        )?
        .print(ctx)
}

impl Project for UnitTest {
    fn get_name(&self) -> &'static str {
        "unittests"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        error!("Should not be called")
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        let test_path = ctx.get_test_path(self);
        print_verbose!("'{}'", file_name(&test_path));
        let config = read_file(&test_path.join("config"))?;
        let mut lines = config.lines();
        let Some(ninja_generator) = lines.next() else {
            return error!("Could not get ninja_generator from config file");
        };
        let targets_to_gen = lines.map(|target| target!(target)).collect::<Vec<_>>();
        match ninja_generator {
            "cmake" => generate_package(
                parse_build_ninja::<CmakeNinjaTarget>(&test_path)?,
                &targets_to_gen,
                &test_path,
                self,
                ctx,
            ),
            "meson" => generate_package(
                parse_build_ninja::<MesonNinjaTarget>(&test_path)?,
                &targets_to_gen,
                &test_path,
                self,
                ctx,
            ),
            "gn" => generate_package(
                parse_build_ninja::<GnNinjaTarget>(&test_path)?,
                &targets_to_gen,
                &test_path,
                self,
                ctx,
            ),
            _ => return error!("Unknown Ninja Generator"),
        }
    }
}
