// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct SpirvTools {
    build_path: PathBuf,
    spirv_headers_path: PathBuf,
    gen_deps: Vec<String>,
}

impl Project for SpirvTools {
    fn get_name(&self) -> &'static str {
        "SPIRV-Tools"
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
        self.build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = PathBuf::from("SPIRV-Tools-ndk");
        self.spirv_headers_path = ProjectId::SpirvHeaders.get_android_path(projects_map, ctx)?;

        common::gen_ninja(
            &src_path,
            &self.build_path,
            vec![path_to_string(&self.spirv_headers_path)],
            ctx,
            self,
        )?;

        const GENERATED_TABLES_BODY: &str = "SPIRV-Tools_core_tables_body";
        const GENERATED_TABLES_HEADER: &str = "SPIRV-Tools_core_tables_header";
        let mut package = SoongPackage::new(
            &[],
            "SPIRV-Tools_license",
            &["SPDX-license-identifier-Apache-2.0"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&Dep::SpirvToolsTargets.get_ninja_targets(projects_map)?)
                .push(target!("core_tables_body.inc", GENERATED_TABLES_BODY))
                .push(target!("core_tables_header.inc", GENERATED_TABLES_HEADER)),
            parse_build_ninja::<CmakeNinjaTarget>(&self.build_path)?,
            &src_path,
            &ndk_path,
            &self.build_path,
            None,
            self,
            ctx,
        )?
        .add_visibilities(Dep::SpirvToolsTargets.get_visibilities(projects_map)?)
        .add_visibilities(vec![String::from("//external/vulkan-shader-profiler")])
        .add_module(SoongModule::new_cc_library_headers(
            CcLibraryHeaders::SpirvTools,
            vec![String::from("include")],
        ));
        self.gen_deps = package
            .get_dep_custom_cmd_inputs()
            .into_iter()
            .map(|header| path_to_string(strip_prefix(header, &self.spirv_headers_path)))
            .collect();

        package
            .add_raw_suffix(&format!(
                r#"
cc_library_headers {{
    name: "SPIRV-Tools-sources",
    header_libs: ["{0}"],
    generated_headers: [
        "{GENERATED_TABLES_BODY}",
        "{GENERATED_TABLES_HEADER}",
    ],
    export_include_dirs: ["."],
    export_header_lib_headers: ["{0}"],
    export_generated_headers: [
        "{GENERATED_TABLES_BODY}",
        "{GENERATED_TABLES_HEADER}",
    ],
    vendor_available: true,
}}
"#,
                CcLibraryHeaders::SpirvHeadersUnified1.str()
            ))
            .print(ctx)
    }

    fn get_deps(&self, _dep: Dep) -> Vec<NinjaTargetToGen> {
        self.gen_deps
            .iter()
            .map(|header| target!(header.as_str()))
            .collect()
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        if !target.ends_with("spirv-as") {
            module = module
                .extend_prop("export_include_dirs", vec!["include"])?
                .add_prop(
                    "export_header_lib_headers",
                    SoongProp::VecStr(vec![CcLibraryHeaders::SpirvHeaders.str()]),
                )
                .add_prop("vendor_available", SoongProp::Bool(true));
            if target.ends_with("libSPIRV-Tools.a") {
                module = module.add_prop("host_supported", SoongProp::Bool(true));
            }
        }
        module
            .add_prop(
                "header_libs",
                SoongProp::VecStr(vec![CcLibraryHeaders::SpirvHeaders.str()]),
            )
            .extend_prop("cflags", vec!["-Wno-implicit-fallthrough"])
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
    fn extend_python_binary_host(
        &self,
        _python_binary_path: &Path,
        module: SoongModule,
    ) -> Result<SoongModule, String> {
        Ok(module.extend_prop("srcs", vec!["utils/Table/*.py"])?)
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

    fn filter_cflag(&self, _cflag: &str) -> bool {
        false
    }
    fn filter_include(&self, include: &Path) -> bool {
        !(include.starts_with(&self.build_path) || include.starts_with(&self.spirv_headers_path))
    }
    fn filter_define(&self, _define: &str) -> bool {
        false
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_lib(&self, lib: &str) -> bool {
        lib != "librt"
    }
}
