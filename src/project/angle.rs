// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Angle {
    src_path: PathBuf,
    build_path: PathBuf,
}

const TARGET_SDK_VERSION: u32 = 35;
const MIN_SDK_VERSION: u32 = 28;

const TARGETS: [&str; 3] = ["libEGL_angle", "libGLESv2_angle", "libGLESv1_CM_angle"];

impl Angle {
    fn filter_path(&self, src: &Path) -> bool {
        for ignore_path in [
            "buildtools",
            "third_party/cpu_features",
            "third_party/libc++",
            "third_party/libc++abi",
            "third_party/libunwind",
            "third_party/llvm-libc",
            "third_party/spirv-headers",
            "third_party/spirv-tools",
            "third_party/zlib",
        ] {
            if src.starts_with(self.src_path.join(ignore_path)) {
                return false;
            }
        }
        if src.starts_with(self.build_path.join("gen")) || src.starts_with("gen") {
            return false;
        }
        true
    }
    fn generate_package_for_target_cpu(
        &mut self,
        ctx: &Context,
        ndk_path: &Path,
        target_cpu: &str,
    ) -> Result<SoongPackage, String> {
        self.build_path = ctx.get_temp_path(&Path::new(self.get_name()).join(target_cpu))?;
        common::gen_ninja(
            &self.src_path,
            &self.build_path,
            vec![
                path_to_string(ctx.get_test_path(self)),
                String::from(target_cpu),
                String::from(if ctx.skip_build {
                    "skip_build"
                } else {
                    "build"
                }),
            ],
            ctx,
            self,
        )?;

        let targets_so = TARGETS
            .iter()
            .map(|target| (String::from("./") + target + ".so", String::from(*target)))
            .collect::<Vec<_>>();
        let mut targets = targets_so
            .iter()
            .map(|(target_so, target)| target!(target_so, target))
            .collect::<Vec<_>>();
        targets.push(target!(
            "./libangle_end2end_tests__library.so",
            "libangle_end2end_tests__library"
        ));
        SoongPackage::default().generate(
            NinjaTargetsToGenMap::from(&targets),
            parse_build_ninja::<GnNinjaTarget>(&self.build_path)?,
            &self.src_path,
            ndk_path,
            &self.build_path,
            None,
            self,
            ctx,
        )
    }
}

impl Project for Angle {
    fn get_name(&self) -> &'static str {
        "angle"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        self.src_path = if let Ok(path) = std::env::var("N2S_ANGLE_PATH") {
            PathBuf::from(path)
        } else {
            PathBuf::from("/ninja-to-soong-angle")
        };
        let ndk_path = self.src_path.join("third_party/android_toolchain/ndk");

        let package = SoongPackageMerger::new(
            ["arm64", "arm", "x64", "x86"]
                .into_iter()
                .map(|target_cpu| {
                    (
                        target_cpu,
                        self.generate_package_for_target_cpu(ctx, &ndk_path, target_cpu),
                    )
                })
                .collect(),
            SoongPackage::new(
                &["//visibility:public"],
                "external_angle_license",
                &[
                    "SPDX-license-identifier-Apache-2.0",
                    "SPDX-license-identifier-BSD",
                    "SPDX-license-identifier-GPL",
                    "SPDX-license-identifier-GPL-2.0",
                    "SPDX-license-identifier-GPL-3.0",
                    "SPDX-license-identifier-LGPL",
                    "SPDX-license-identifier-MIT",
                    "SPDX-license-identifier-Zlib",
                    "legacy_unencumbered",
                ],
                &[
                    "LICENSE",
                    "src/common/third_party/xxhash/LICENSE",
                    "src/libANGLE/renderer/vulkan/shaders/src/third_party/ffx_spd/LICENSE",
                    "src/tests/test_utils/third_party/LICENSE",
                    "src/third_party/libXNVCtrl/LICENSE",
                    "src/third_party/volk/LICENSE.md",
                    "third_party/abseil-cpp/LICENSE",
                    "third_party/android_system_sdk/LICENSE",
                    "third_party/bazel/LICENSE",
                    "third_party/colorama/LICENSE",
                    "third_party/glslang/LICENSE",
                    "third_party/glslang/src/LICENSE.txt",
                    "third_party/proguard/LICENSE",
                    "third_party/r8/LICENSE",
                    "third_party/spirv-headers/LICENSE",
                    "third_party/spirv-headers/src/LICENSE",
                    "third_party/spirv-tools/LICENSE",
                    "third_party/spirv-tools/src/LICENSE",
                    "third_party/spirv-tools/src/utils/vscode/src/lsp/LICENSE",
                    "third_party/turbine/LICENSE",
                    "third_party/vulkan-headers/LICENSE.txt",
                    "third_party/vulkan-headers/src/LICENSE.md",
                    "third_party/vulkan_memory_allocator/LICENSE.txt",
                    "tools/flex-bison/third_party/m4sugar/LICENSE",
                    "tools/flex-bison/third_party/skeletons/LICENSE",
                    "util/windows/third_party/StackWalker/LICENSE",
                ],
            ),
        )?
        .merge()?;

        let default_module =
            SoongModule::new_cc_defaults(CcDefaults::Angle).add_props(package.get_props(
                "angle_obj_libpreprocessor_a",
                vec!["cflags", "local_include_dirs", "shared_libs", "stl", "arch"],
            )?);

        package
            .add_module(default_module)
            .add_raw_suffix(&format!(
                r#"
soong_config_module_type {{
    name: "angle_config_cc_defaults",
    module_type: "cc_defaults",
    config_namespace: "angle",
    bool_variables: [
        "angle_in_vendor",
    ],
    properties: [
        "target.android.relative_install_path",
        "vendor",
    ],
}}

soong_config_bool_variable {{
    name: "angle_in_vendor",
}}

angle_config_cc_defaults {{
    name: "{0}",
    vendor: false,
    target: {{
        android: {{
            relative_install_path: "",
        }},
    }},
    soong_config_variables: {{
        angle_in_vendor: {{
            vendor: true,
            target: {{
                android: {{
                    relative_install_path: "egl",
                }},
            }},
        }},
    }},
}}

filegroup {{
    name: "ANGLE_srcs",
    srcs: [
        "src/android_system_settings/src/com/android/angle/MainActivity.java",
        "src/android_system_settings/src/com/android/angle/common/AngleRuleHelper.java",
        "src/android_system_settings/src/com/android/angle/common/GlobalSettings.java",
        "src/android_system_settings/src/com/android/angle/common/MainFragment.java",
        "src/android_system_settings/src/com/android/angle/common/Receiver.java",
        "src/android_system_settings/src/com/android/angle/common/SearchProvider.java",
    ],
}}

prebuilt_etc {{
    name: "android.software.angle.xml",
    src: "android/android.software.angle.xml",
    product_specific: true,
    sub_dir: "permissions",
}}

java_defaults {{
    name: "ANGLE_java_defaults",
    sdk_version: "system_current",
    target_sdk_version: "{TARGET_SDK_VERSION}",
    min_sdk_version: "{MIN_SDK_VERSION}",
    compile_multilib: "both",
    use_embedded_native_libs: true,
    jni_libs: [
{1}
    ],
    aaptflags: [
        "--extra-packages com.android.angle.common",
        "-0 .json",
    ],
    srcs: [
        ":ANGLE_srcs",
    ],
    privileged: true,
    product_specific: true,
    owner: "google",
    required: [
        "android.software.angle.xml",
    ],
}}

android_library {{
    name: "ANGLE_library",
    sdk_version: "system_current",
    target_sdk_version: "{TARGET_SDK_VERSION}",
    min_sdk_version: "{MIN_SDK_VERSION}",
    resource_dirs: [
        "src/android_system_settings/res",
    ],
    asset_dirs: [
        "src/android_system_settings/assets",
    ],
    aaptflags: [
        "-0 .json",
    ],
    manifest: "src/android_system_settings/src/com/android/angle/AndroidManifest.xml",
    static_libs: [
        "androidx.preference_preference",
    ],
}}

android_app {{
    name: "ANGLE",
    defaults: [
        "ANGLE_java_defaults",
    ],
    manifest: "src/android_system_settings/src/com/android/angle/AndroidManifest.xml",
    static_libs: [
        "ANGLE_library",
    ],
    optimize: {{
        enabled: true,
        shrink: true,
        proguard_compatibility: false,
    }},
    asset_dirs: [
        "src/android_system_settings/assets",
    ],
}}

java_defaults {{
    name: "ANGLE_java_settings_defaults",
    sdk_version: "system_current",
    target_sdk_version: "{TARGET_SDK_VERSION}",
    min_sdk_version: "{MIN_SDK_VERSION}",
    compile_multilib: "both",
    use_embedded_native_libs: true,
    aaptflags: [
        "--extra-packages com.android.angle.common",
        "-0 .json",
    ],
    srcs: [
        ":ANGLE_srcs",
    ],
    privileged: true,
    product_specific: true,
    owner: "google",
    required: [
        "android.software.angle.xml",
    ],
}}

android_app {{
    name: "ANGLE_settings",
    defaults: [
        "ANGLE_java_settings_defaults",
    ],
    manifest: "src/android_system_settings/src/com/android/angle/AndroidManifest.xml",
    static_libs: [
        "ANGLE_library",
    ],
    optimize: {{
        enabled: true,
        shrink: true,
        proguard_compatibility: false,
    }},
    asset_dirs: [
        "src/android_system_settings/assets",
    ],
}}
        "#,
                CcDefaults::AngleVendor.str(),
                TARGETS
                    .iter()
                    .map(|target| String::from("        \"") + target + "\",")
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
            .print(ctx)
    }

    fn extend_module(&self, target: &Path, mut module: SoongModule) -> Result<SoongModule, String> {
        let target_name_holder = file_name(target);
        let target_name = &target_name_holder.as_str();
        if target.ends_with("libtranslator.a") {
            module = module.add_prop(
                "header_libs",
                SoongProp::VecStr(vec![
                    CcLibraryHeaders::SpirvHeaders.str(),
                    CcLibraryHeaders::SpirvTools.str(),
                ]),
            );
        }
        if !["libGLESv1_CM_angle.so", "libgtest.a"].contains(target_name) {
            module = module.add_defaults(CcDefaults::Angle)?;
        }
        if TARGETS.contains(target_name) {
            module = module.add_defaults(CcDefaults::AngleVendor)?;
        }
        let mut libs = Vec::new();
        if target.starts_with("obj") {
            libs.push("libnativewindow");
        } else if target.ends_with("libGLESv2_angle.so") {
            libs.push("libz");
        }
        module
            .add_prop("stl", SoongProp::Str(String::from("libc++_static")))
            .extend_prop(
                "cflags",
                vec![
                    "-Wno-nullability-completeness",
                    "-O2",
                    "-fno-stack-protector",
                    "-fno-unwind-tables",
                ],
            )?
            .extend_prop("shared_libs", libs)
    }

    fn map_cmd_output(&self, output: &Path) -> Option<String> {
        Some(file_name(output))
    }
    fn map_lib(&self, library: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        if library.starts_with("obj/third_party/spirv-tools") {
            Some((PathBuf::from("SPIRV-Tools/source/libSPIRV-Tools.a"), kind))
        } else if library.starts_with("obj/third_party/zlib") {
            Some((
                PathBuf::from("zlib_google_compression_utils_portable"),
                kind,
            ))
        } else if library.starts_with("obj/third_party/cpu_features") {
            Some((PathBuf::from("cpufeatures"), kind))
        } else if library.starts_with("obj") {
            None
        } else {
            Some((PathBuf::from(library), kind))
        }
    }

    fn filter_cflag(&self, cflag: &str) -> bool {
        cflag.starts_with("-fvisibility")
    }
    fn filter_gen_header(&self, header: &Path) -> bool {
        header.starts_with("gen/angle")
    }
    fn filter_include(&self, include: &Path) -> bool {
        self.filter_path(include)
    }
    fn filter_lib(&self, lib: &str) -> bool {
        !lib.contains("llvm-build/Release+Asserts")
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_source(&self, source: &Path) -> bool {
        self.filter_path(source)
    }
    fn filter_target(&self, target: &Path) -> bool {
        !(target.starts_with("obj/third_party")
            || target.starts_with("gen/third_party/spirv-tools"))
    }
}
