// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct MediaDriver {
    src_path: PathBuf,
}

impl Project for MediaDriver {
    fn get_name(&self) -> &'static str {
        "media-driver"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("vendor/intel/desktop").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        self.src_path = ctx.get_android_path(self)?;
        let build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;

        common::gen_ninja(
            &self.src_path,
            &build_path,
            vec![path_to_string(&ndk_path)],
            ctx,
            self,
        )?;

        SoongPackage::new(
            &[],
            "external_intel_media_driver_license",
            &[],
            &["LICENSE.md"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[target!(
                "media_driver/iHD_drv_video.so",
                "iHD_drv_video"
            )]),
            parse_build_ninja::<CmakeNinjaTarget>(&build_path)?,
            &self.src_path,
            &ndk_path,
            &build_path,
            None,
            self,
            ctx,
        )?
        .add_raw_suffix(&format!(
            r#"
cc_defaults {{
    name: "{}",
    cflags: [
        "-Wno-extern-initializer",
        "-Wno-ignored-qualifiers",
        "-Wno-implicit-fallthrough",
        "-Wno-pragma-pack-suspicious-include",
        // Driver has #if LINUX guards; NDK configure does not set it.
        "-DLINUX",
        // Driver has #ifdef ANDROID guards; NDK defines only __ANDROID__.
        "-DANDROID=1",
        // Legacy value from the Android.mk template.
        "-DANDROID_VERSION=800",
    ] + select(product_variable("debuggable"), {{
        // userdebug/eng -> _RELEASE_INTERNAL, user -> _RELEASE.
        true: ["-D_RELEASE_INTERNAL", "-DSYSCONFDIR=\"/vendor/etc\""],
        false: ["-D_RELEASE"],
    }}),
    conlyflags: ["-xc++"],
    c_std: "c++14",
    cpp_std: "c++14",
    header_libs: [
        "libva_headers",
        "libigdgmm_headers",
        "libcmrt_headers",
    ],
    shared_libs: [
        "libutils",
        "liblog",
    ],
    required: [
        "libdrm",
        "libva",
    ],
    rtti: true,
    enabled: false,
    arch: {{
        x86_64: {{
            enabled: true,
        }},
    }},
    vendor: true,
}}
"#,
            CcDefaults::MediaDriver.str()
        ))
        .print(ctx)
    }

    fn extend_module(&self, _target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        Ok(module.add_defaults(CcDefaults::MediaDriver)?)
    }

    fn map_lib(&self, lib: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        if lib.ends_with("libigdgmm") {
            return Some((PathBuf::from("libigdgmm_android"), kind));
        }
        None
    }

    fn filter_cflag(&self, cflag: &str) -> bool {
        cflag.starts_with("-W") || cflag == "-fexceptions"
    }
    fn filter_define(&self, define: &str) -> bool {
        // Variant is chosen at Soong time by the select() in media-driver-defaults.
        define != "_RELEASE" && define != "_RELEASE_INTERNAL"
    }
    fn filter_include(&self, include: &Path) -> bool {
        include.starts_with(&self.src_path)
            && !include.starts_with(self.src_path.join(".."))
            && !include.starts_with(self.src_path.join("cmrtlib"))
            && include != self.src_path.join("media_softlet/linux/common/cp")
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
}
