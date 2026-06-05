// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

const SPIRV_AS: &str = "spirv-as";

#[derive(Default)]
pub struct OpenclCts {
    src_path: PathBuf,
    build_path: PathBuf,
    spirv_headers_path: PathBuf,
    gen_deps: Vec<String>,
}

const SPIRV_NEW_DATA: &str = "OpenCL-CTS-spirv_new_data";
const SPIR_DATA: &str = "OpenCL-CTS-spir_data";
const COMPILER_DATA: &str = "OpenCL-CTS-compiler_data";

fn parse_test(line: &str) -> Option<String> {
    let split_comma = line.split(",");
    let Some(cmd) = split_comma.last() else {
        return None;
    };
    let mut split_space = cmd.trim().split(" ");
    let Some(binary) = split_space.next() else {
        return None;
    };
    Some(String::from("test_conformance/") + binary)
}

fn parse_tests(file_path: &Path) -> Result<Vec<String>, String> {
    let mut tests = Vec::new();
    let content = read_file(file_path)?;
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        if line.is_empty() || line.starts_with("#") || line.starts_with("OpenCL-GL") {
            continue;
        }
        if let Some(test) = parse_test(line) {
            tests.push(test);
        }
    }
    tests.sort_unstable();
    tests.dedup();

    Ok(tests)
}

impl Project for OpenclCts {
    fn get_name(&self) -> &'static str {
        "OpenCL-CTS"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        self.src_path = ctx.get_android_path(self)?;
        self.build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;
        self.spirv_headers_path = ProjectId::SpirvHeaders.get_android_path(projects_map, ctx)?;

        common::gen_ninja(
            &self.src_path,
            &self.build_path,
            vec![
                path_to_string(&ndk_path),
                path_to_string(&self.spirv_headers_path),
            ],
            ctx,
            self,
        )?;

        const CSV_FILENAME: &str = "opencl_conformance_tests_full.csv";
        let csv_file_dst_path = self.build_path.join(CSV_FILENAME);
        let csv_file_src_path = self.src_path.join("test_conformance").join(CSV_FILENAME);
        if csv_file_src_path.exists() {
            write_file(&csv_file_dst_path, &read_file(&csv_file_src_path)?)?;
        }
        let tests = parse_tests(&csv_file_dst_path)?
            .into_iter()
            .map(|test| {
                (
                    test.clone(),
                    String::from(self.get_name()) + "-" + test.split("/").last().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let targets = tests
            .iter()
            .map(|(test, name)| target_typed!(test, "cc_test", name))
            .collect::<Vec<_>>();
        let mut package = SoongPackage::new(
            &["//visibility:public"],
            "external_OpenCL-CTS_license",
            &[
                "SPDX-license-identifier-Apache-2.0",
                "SPDX-license-identifier-BSD",
                "SPDX-license-identifier-MIT",
                "SPDX-license-identifier-Unlicense",
            ],
            &["LICENSE.txt"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&targets),
            parse_build_ninja::<CmakeNinjaTarget>(&self.build_path)?,
            &self.src_path,
            &ndk_path,
            &self.build_path,
            None,
            self,
            ctx,
        )?;

        let gen_deps = package.get_dep_gen_assets();
        let mut spirv_new_data = Vec::new();
        for dep in gen_deps {
            if dep.ends_with("spirv.core.grammar.json") {
                continue;
            }
            let folder = file_name(dep.clone().parent().unwrap());
            let basename = PathBuf::from("test_conformance/spirv_new/spirv_asm");
            let (target_env, dirname) = if folder.starts_with("spv") {
                (folder.clone(), basename.join(folder))
            } else {
                (String::from("spv1.0"), basename)
            };
            let source = dirname.join(file_name(&dep).replace(".spv", ".spvasm"));
            let name = String::from(self.get_name())
                + "-"
                + &path_to_id(strip_prefix(
                    dep.clone(),
                    "test_conformance/spirv_new/spirv_bin",
                ));
            spirv_new_data.push(String::from(":") + &name);
            package = package.add_module(
                SoongModule::new("gensrcs")
                    .add_prop("name", SoongProp::Str(name))
                    .add_prop(
                        "cmd",
                        SoongProp::Str(format!(
                            "$(location) --target-env {target_env} $(in) -o $(out)"
                        )),
                    )
                    .add_prop("srcs", SoongProp::VecStr(vec![path_to_string(source)]))
                    .add_prop("output_extension", SoongProp::Str(file_ext(&dep)))
                    .add_prop("tools", SoongProp::VecStr(vec![String::from(SPIRV_AS)])),
            );
        }
        self.gen_deps = package
            .get_dep_gen_assets()
            .into_iter()
            .filter_map(|dep| {
                if let Ok(strip) = dep.strip_prefix(&self.spirv_headers_path) {
                    return Some(path_to_string(strip));
                }
                None
            })
            .collect();

        let default_module = SoongModule::new_cc_defaults(CcDefaults::OpenclCts)
            .add_props(package.get_props(
                "OpenCL-CTS-test_api",
                vec!["cflags", "local_include_dirs", "static_libs", "shared_libs"],
            )?)
            .add_defaults(CcDefaults::OpenclCtsManual)?;
        package
            .add_module(SoongModule::new_filegroup(
                String::from(SPIRV_NEW_DATA),
                spirv_new_data,
            ))
            .add_module(SoongModule::new_filegroup(
                String::from(SPIR_DATA),
                vec![String::from("test_conformance/spir/*.zip")],
            ))
            .add_module(SoongModule::new_filegroup(
                String::from(COMPILER_DATA),
                vec![
                    String::from(
                        "test_conformance/compiler/includeTestDirectory/testIncludeFile.h",
                    ),
                    String::from(
                        "test_conformance/compiler/secondIncludeTestDirectory/testIncludeFile.h",
                    ),
                ],
            ))
            .add_module(default_module)
            .add_raw_suffix(&format!(
                r#"
cc_defaults {{
    name: "{3}",
    header_libs: [
        "OpenCL-Headers",
        "{2}",
    ],
    cflags: [
        "-Wno-error",
        "-Wno-c++11-narrowing",
        "-Wno-non-virtual-dtor",
        "-Wno-string-concatenation",
        "-fexceptions",
    ],
    gtest: false,
    test_options: {{
        unit_test: false,
    }},
}}

cc_test {{
    name: "{0}",
    data: [
{1}
        ":{COMPILER_DATA}",
        ":{SPIR_DATA}",
        ":{SPIRV_NEW_DATA}",
    ],
    test_options: {{
        unit_test: false,
    }},
    test_config: "android/{0}.xml",
}}
"#,
                self.get_name(),
                tests
                    .iter()
                    .map(|(_, test)| String::from("        \":") + test + "\",")
                    .collect::<Vec<_>>()
                    .join("\n"),
                CcLibraryHeaders::SpirvHeaders.str(),
                CcDefaults::OpenclCtsManual.str()
            ))
            .print(ctx)
    }

    fn get_deps(&self, dep: Dep) -> Vec<NinjaTargetToGen> {
        match dep {
            Dep::SpirvToolsTargets => {
                vec![target_typed!("tools/spirv-as", "cc_binary_host", SPIRV_AS)]
            }
            Dep::SpirvHeaders => self.gen_deps.iter().map(|lib| target!(lib)).collect(),
            _ => Vec::new(),
        }
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        let is_test_spir = target.ends_with("test_spir");
        let data = if target.ends_with("test_compiler") {
            COMPILER_DATA
        } else if is_test_spir {
            SPIR_DATA
        } else if target.ends_with("test_spirv_new") {
            SPIRV_NEW_DATA
        } else {
            ""
        };
        let defaults = if target.ends_with("libharness.a") {
            CcDefaults::OpenclCtsManual
        } else {
            module = module.add_prop(
                "test_config",
                SoongProp::Str(
                    String::from("android/") + self.get_name() + "-" + &file_name(target) + ".xml",
                ),
            );
            CcDefaults::OpenclCts
        };
        module = module
            .add_prop("rtti", SoongProp::Bool(is_test_spir))
            .add_defaults(defaults)?;
        if !data.is_empty() {
            let data_str = format!(":{data}");
            module = module.extend_prop("data", vec![&data_str])?;
        }
        Ok(module)
    }
    fn extend_custom_command(
        &self,
        _target: &Path,
        module: SoongModule,
    ) -> Result<SoongModule, String> {
        Ok(module
            .add_prop("vendor_available", SoongProp::Bool(true))
            .add_prop("host_supported", SoongProp::Bool(true)))
    }

    fn map_cmd_input(&self, input: &Path) -> Option<String> {
        if input.starts_with(&self.spirv_headers_path) {
            return Some(Dep::SpirvHeaders.get_id(
                input,
                &self.spirv_headers_path,
                &self.build_path,
            ));
        }
        None
    }
    fn map_cmd_output(&self, output: &Path) -> Option<String> {
        Some(file_name(output))
    }
    fn map_lib(&self, lib: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        if lib.ends_with("libOpenCL") {
            return Some((
                PathBuf::from("//external/OpenCL-ICD-Loader:libOpenCL"),
                kind,
            ));
        }
        None
    }

    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_gen_header(&self, header: &Path) -> bool {
        header.ends_with("spirv_capability_deps.def")
    }
    fn filter_include(&self, include: &Path) -> bool {
        !include.ends_with("OpenCL-Headers")
            && path_to_string(include).starts_with(&path_to_string(&self.src_path))
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_target(&self, target: &Path) -> bool {
        !target.starts_with("test_conformance/spirv_new/spirv_bin")
    }
}
