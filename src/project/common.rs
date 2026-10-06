// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub const LLVM_PROJECT_NAME: &str = "llvm-project";

pub fn copy_gen_deps(
    gen_deps: Vec<PathBuf>,
    from: &str,
    build_path: &Path,
    ctx: &Context,
    project: &dyn Project,
) -> Result<(), String> {
    if !ctx.copy_to_aosp {
        write_file(
            &ctx.get_test_path(project).join("generated_deps.txt"),
            &format!("{0:#?}", &gen_deps),
        )?;
    } else {
        let dst = ctx.get_android_path(project)?.join(from);
        if remove_dir(&dst)? {
            print_verbose!("{dst:#?} removed");
        }
        for file in gen_deps {
            let from = build_path.join(&file);
            let to = dst.join(&file);
            let to_path = to.parent().unwrap();
            create_dir(to_path)?;
            copy_file(&from, &to)?;
        }
        print_verbose!("Files copied from {build_path:#?} to {dst:#?}");
    }
    Ok(())
}

pub fn ninja_build(build_path: &Path, targets: &Vec<PathBuf>, ctx: &Context) -> Result<(), String> {
    if ctx.skip_build {
        return Ok(());
    }
    let mut args = vec![String::from("-C"), path_to_string(&build_path)];
    for target in targets {
        args.push(path_to_string(target));
    }
    let args: Vec<_> = args.iter().map(|target| target.as_str()).collect();
    execute_cmd!("ninja", &args)
}

pub fn gen_ninja(
    src_path: &Path,
    build_path: &Path,
    other_args: &[impl AsRef<Path>],
    ctx: &Context,
    project: &dyn Project,
) -> Result<(), String> {
    if ctx.skip_gen_ninja && build_path.exists() {
        return Ok(());
    }
    let mut args = vec![
        path_to_string(ctx.get_script_path(project).join("gen-ninja.sh")),
        path_to_string(src_path),
        path_to_string(build_path),
    ];
    for arg in other_args {
        args.push(path_to_string(arg));
    }
    let args = args.iter().map(|arg| arg.as_str()).collect::<Vec<_>>();
    execute_cmd!("bash", &args)
}
