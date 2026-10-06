// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

const MESON_GENERATED: &str = "meson_generated";

#[derive(Default)]
pub struct Fwupd {
    src_path: PathBuf,
    build_path: PathBuf,
}

impl Project for Fwupd {
    fn get_name(&self) -> &'static str {
        "fwupd"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        self.src_path = if ctx.copy_to_aosp {
            ctx.get_android_path(self)?
        } else {
            PathBuf::from("/ninja-to-soong-fwupd")
        };
        self.build_path = ctx.get_temp_path(Path::new(self.get_name()))?;
        let ndk_path = get_ndk_path(ctx)?;

        common::gen_ninja(
            &self.src_path,
            &self.build_path,
            &[
                ndk_path.clone(),
                ctx.get_test_path(self),
                PathBuf::from(if ctx.copy_to_aosp { "copy_to_aosp" } else { "" }),
            ],
            ctx,
            self,
        )?;

        let mut package = SoongPackage::new(
            &["//visibility:public"],
            "fwupd_license",
            &["SPDX-license-identifier-LGPL-2.1"],
            &["LICENSE"],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[
                target!("src/fwupdmgr", "fwupdmgr"),
                target!("src/fwupd-binder", "fwupd-binder"),
            ]),
            parse_build_ninja::<MesonNinjaTarget>(&self.build_path)?,
            &self.src_path,
            &ndk_path,
            &self.build_path,
            Some(MESON_GENERATED),
            self,
            ctx,
        )?;

        let mut gen_deps = package.get_dep_gen_assets();
        common::ninja_build(&self.build_path, &gen_deps, ctx)?;
        gen_deps.extend(
            [
                "config.h",
                "libfwupd/fwupd-version.h",
                "subprojects/curl-8.10.1/lib/curl_config.h",
                "subprojects/glib-2.84.2/config.h",
                "subprojects/glib-2.84.2/gio/gnetworking.h",
                "subprojects/glib-2.84.2/glib/glibconfig.h",
                "subprojects/glib-2.84.2/glib/gnulib/gnulib_math.h",
                "subprojects/glib-2.84.2/gmodule/gmoduleconf.h",
                "subprojects/json-glib/json-glib/json-version.h",
                "subprojects/libffi/fficonfig.h",
                "subprojects/libffi/include/ffi.h",
                "subprojects/libffi/include/ffitarget.h",
                "subprojects/libffi/include/ffitarget-x86_64.h",
                "subprojects/libffi/include/ffi-x86_64.h",
                "subprojects/libjcat/libjcat/jcat-version.h",
                "subprojects/libusb-1.0.27/config.h",
                "subprojects/libxmlb/src/libxmlb/xb-builder.h",
                "subprojects/libxmlb/src/libxmlb/xb-builder-fixup.h",
                "subprojects/libxmlb/src/libxmlb/xb-builder-node.h",
                "subprojects/libxmlb/src/libxmlb/xb-builder-source.h",
                "subprojects/libxmlb/src/libxmlb/xb-builder-source-ctx.h",
                "subprojects/libxmlb/src/libxmlb/xb-compile.h",
                "subprojects/libxmlb/src/libxmlb/xb-machine.h",
                "subprojects/libxmlb/src/libxmlb/xb-node.h",
                "subprojects/libxmlb/src/libxmlb/xb-node-query.h",
                "subprojects/libxmlb/src/libxmlb/xb-node-silo.h",
                "subprojects/libxmlb/src/libxmlb/xb-opcode.h",
                "subprojects/libxmlb/src/libxmlb/xb-query.h",
                "subprojects/libxmlb/src/libxmlb/xb-query-context.h",
                "subprojects/libxmlb/src/libxmlb/xb-silo-export.h",
                "subprojects/libxmlb/src/libxmlb/xb-silo.h",
                "subprojects/libxmlb/src/libxmlb/xb-silo-query.h",
                "subprojects/libxmlb/src/libxmlb/xb-stack.h",
                "subprojects/libxmlb/src/libxmlb/xb-string.h",
                "subprojects/libxmlb/src/libxmlb/xb-value-bindings.h",
                "subprojects/libxmlb/src/libxmlb/xb-version.h",
                "subprojects/pcre2-10.44/config.h",
                "subprojects/pcre2-10.44/pcre2.h",
                "subprojects/pcre2-10.44/pcre2_chartables.c",
                "subprojects/xz-5.2.12/config.h",
            ]
            .map(|dep| PathBuf::from(dep)),
        );
        package.filter_gen_deps(MESON_GENERATED, &gen_deps)?;
        common::copy_gen_deps(gen_deps, MESON_GENERATED, &self.build_path, ctx, self)?;
        package.print(ctx)
    }

    fn extend_module(&self, target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        if target.ends_with("libxmlb.so") {
            module.extend_prop(
                "local_include_dirs",
                vec!["meson_generated/subprojects/libxmlb/src/libxmlb"],
            )?
        } else if target.ends_with("fwupd-binder") {
            module.extend_prop(
                "local_include_dirs",
                vec!["subprojects/android_frameworks/libs/binder/ndk/include_ndk"],
            )?
        } else {
            module
        }
        .extend_prop(
            "cflags",
            vec![
                "-Wno-macro-redefined",
                "-Wno-pointer-sign",
                "-Wno-incompatible-pointer-types-discards-qualifiers",
            ],
        )
    }

    fn map_lib(&self, lib: &Path, kind: LibraryKind) -> Option<(PathBuf, LibraryKind)> {
        if lib.ends_with("libbinder_ndk.so") {
            return Some((PathBuf::from("libbinder_ndk"), kind));
        } else if lib.ends_with("libz") {
            return Some((PathBuf::from("libz"), kind));
        }
        None
    }

    fn filter_cflag(&self, cflag: &str) -> bool {
        cflag.starts_with("-Wno")
    }
    fn filter_define(&self, define: &str) -> bool {
        define != "HAVE_MALLOC_USABLE_SIZE"
    }
    fn filter_gen_header(&self, _header: &Path) -> bool {
        false
    }
    fn filter_gen_source(&self, _source: &Path) -> bool {
        false
    }
    fn filter_include(&self, include: &Path) -> bool {
        include != self.build_path.join("subprojects/libxmlb/src/libxmlb")
    }
    fn filter_link_flag(&self, _flag: &str) -> bool {
        false
    }
    fn filter_target(&self, target: &Path) -> bool {
        !["c", "h", "def", "xml"].contains(&file_ext(target).as_str())
    }
}
