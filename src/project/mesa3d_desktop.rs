// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

const MESA_PYTHON_DEFAULT: &str = "mesa_python_default";

pub trait Mesa3dProject {
    fn get_name(&self) -> &'static str;
    fn get_subprojects_path(&self) -> String;
    fn create_package(
        &mut self,
        ctx: &Context,
        src_path: &Path,
        build_path: &Path,
        ndk_path: &Path,
        meson_generated: &str,
        targets_map: NinjaTargetsMap<MesonNinjaTarget>,
    ) -> Result<SoongPackage, String>;
    fn get_default_module(&self, package: &SoongPackage) -> Result<SoongModule, String>;
    fn get_raw_suffix(&self, common_raw_prop: &'static str) -> String;
    fn extend_module(&self, target: &Path, module: SoongModule) -> Result<SoongModule, String>;
    fn asset_filter(&self, asset: &Path) -> bool;
    fn mesa_filter(&self, asset: &Path) -> bool {
        let str = path_to_string(asset);
        self.asset_filter(asset)
            && !str.contains("libdrm") // dependency
            && !str.starts_with("src/android_stub") // dependencies
            && !str.ends_with("git_sha1.h") // git
    }
    fn extract_assets_to_filter(
        targets: &NinjaTargetsToGenMap,
        targets_map: &NinjaTargetsMap<MesonNinjaTarget>,
    ) -> Result<Vec<PathBuf>, String> {
        let mut assets = Vec::new();
        targets_map.traverse_from(targets.get_targets(), false, |target| {
            if let NinjaRule::CustomCommand(custom_command) = target.get_rule()? {
                if custom_command.command.split(" ").any(|split| {
                    for tool in ["panfrost_compile", "mesa_clc", "vtn_bindgen2"] {
                        if split.ends_with(tool) {
                            return true;
                        }
                    }
                    false
                }) {
                    let mut outputs = target.get_outputs().clone();
                    outputs.extend(target.get_implicit_ouputs().clone());
                    assets.extend(
                        outputs
                            .into_iter()
                            .map(|output| strip_prefix(output, "n2s")),
                    );
                }
            }
            Ok(true)
        })?;
        Ok(assets)
    }
}

impl<T> Project for T
where
    T: Mesa3dProject,
{
    fn get_name(&self) -> &'static str {
        self.get_name()
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("vendor/google/graphics").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        let src_path = ctx.get_android_path(self)?;
        let ndk_path = get_ndk_path(ctx)?;
        let build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let mesa_clc_build_path =
            ctx.get_temp_path(&Path::new("mesa_clc").join(self.get_name()))?;
        let script_path = ctx.get_script_path(self);

        let mesa_clc_path = if !ctx.skip_build {
            execute_cmd!(
                "bash",
                [
                    &path_to_string(script_path.join("build_mesa_clc.sh")),
                    &path_to_string(&src_path),
                    &path_to_string(&mesa_clc_build_path)
                ]
            )?;
            mesa_clc_build_path.join("bin")
        } else {
            script_path.clone()
        };

        common::gen_ninja(
            &src_path,
            &build_path,
            vec![path_to_string(mesa_clc_path), path_to_string(&ndk_path)],
            ctx,
            self,
        )?;

        let targets = parse_build_ninja::<MesonNinjaTarget>(&build_path)?;
        const MESON_GENERATED: &str = "meson_generated";
        let mut package = self.create_package(
            ctx,
            &src_path,
            &build_path,
            &ndk_path,
            MESON_GENERATED,
            NinjaTargetsMap::new(&targets),
        )?;

        let gen_deps = package
            .get_dep_gen_assets()
            .into_iter()
            .filter(|include| !include.starts_with("subprojects"))
            .collect();

        common::ninja_build(&build_path, &gen_deps, ctx)?;
        // Clean libdrm and expat to prevent Soong from parsing blueprints that
        // came with it.
        if !ctx.skip_gen_ninja {
            for libname in ["libdrm", "expat"] {
                execute_cmd!(
                    "git",
                    [
                        "-C",
                        &path_to_string(&src_path),
                        "clean",
                        "-xfd",
                        format!("subprojects/{}*", libname).as_str()
                    ]
                )?;
            }
        }

        package.filter_gen_deps(MESON_GENERATED, &gen_deps)?;
        common::copy_gen_deps(gen_deps, MESON_GENERATED, &build_path, ctx, self)?;
        let default_module = self.get_default_module(&package)?;

        package
            .add_module(default_module)
            .add_raw_suffix(
                &(self.get_raw_suffix(
                    r#"    product_variables: {
        platform_sdk_version: {
            cflags: ["-DANDROID_API_LEVEL=%d"],
        },
    },"#,
                ) + &format!(
                    r#"
python_defaults {{
    name: "{MESA_PYTHON_DEFAULT}",
    libs: [
        "mako",
        "pyyaml",
    ],
}}
"#
                )),
            )
            .add_raw_prefix(
                r#"
soong_namespace {
}
"#,
            )
            .print(ctx)
    }

    fn extend_module(&self, target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        self.extend_module(target, module)
    }
    fn extend_custom_command(
        &self,
        target: &Path,
        mut module: SoongModule,
    ) -> Result<SoongModule, String> {
        if let Some(prop) = module.get_prop("out") {
            if let SoongProp::VecStr(mut outs) = prop.get_prop() {
                if outs.len() == 1 && file_ext(Path::new(&outs[0])).starts_with("h") {
                    let mut target = target.parent().unwrap();
                    let mut cmd_suffix = String::new();
                    while !target.ends_with("src") {
                        let prefix = file_name(&target);
                        target = target.parent().unwrap();
                        let prev_out = outs.last().unwrap();
                        let new_out = path_to_string(Path::new(&prefix).join(prev_out));
                        cmd_suffix = cmd_suffix
                            + " && cp $(location "
                            + prev_out
                            + ") $(location "
                            + &new_out
                            + ")";
                        outs.push(new_out);
                    }
                    module.update_prop("out", |_| Ok(SoongProp::VecStr(outs.clone())))?;
                    module.update_prop("cmd", |prop| {
                        match prop {
                            SoongProp::Str(cmd) => {
                                return Ok(SoongProp::Str(
                                    cmd.replace(
                                        "$(out)",
                                        &(String::from("$(location ") + &outs[0] + ")"),
                                    ) + &cmd_suffix,
                                ));
                            }
                            _ => (),
                        }
                        return Ok(prop);
                    })?;
                }
            }
        }
        Ok(module.add_prop("vendor_available", SoongProp::Bool(true)))
    }
    fn extend_python_binary_host(
        &self,
        _python_binary_path: &Path,
        module: SoongModule,
    ) -> Result<SoongModule, String> {
        Ok(module.add_prop(
            "defaults",
            SoongProp::VecStr(vec![String::from(MESA_PYTHON_DEFAULT)]),
        ))
    }

    fn map_cmd_output(&self, output: &Path) -> Option<String> {
        Some(file_name(output))
    }
    fn map_lib(&self, library: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        if path_to_string(library).starts_with("subprojects/expat") {
            Some((PathBuf::from("libexpat"), LibraryKind::Static))
        } else if library.starts_with("src/android_stub")
            || (!library.starts_with("src") && !library.starts_with("subprojects/perfetto"))
        {
            Some((PathBuf::from(file_stem(library)), kind))
        } else {
            None
        }
    }

    fn filter_define(&self, define: &str) -> bool {
        !define.starts_with("ANDROID_API_LEVEL=")
    }
    fn filter_cflag(&self, cflag: &str) -> bool {
        cflag == "-mclflushopt"
    }
    fn filter_include(&self, include: &Path) -> bool {
        !path_to_string(include).contains(&self.get_subprojects_path())
    }
    fn filter_link_flag(&self, flag: &str) -> bool {
        flag == "-Wl,--build-id=sha1"
    }
    fn filter_gen_header(&self, header: &Path) -> bool {
        self.mesa_filter(header)
    }
    fn filter_gen_source(&self, source: &Path) -> bool {
        self.mesa_filter(source)
    }
    fn filter_target(&self, target: &Path) -> bool {
        self.mesa_filter(target)
    }
}
