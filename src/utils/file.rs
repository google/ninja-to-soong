// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use std::fs::*;

use super::*;

pub fn remove_dir(dir: &Path) -> Result<bool, String> {
    if !dir.exists() {
        return Ok(false);
    }
    if let Err(err) = remove_dir_all(dir) {
        return error!("remove_dir_all({dir:#?}) failed: {err}");
    }
    Ok(true)
}

pub fn create_dir(dir: &Path) -> Result<bool, String> {
    if dir.exists() {
        return Ok(false);
    }
    if let Err(err) = create_dir_all(dir) {
        return error!("create_dir_all({dir:#?}) failed: '{err}'");
    }
    Ok(true)
}

pub fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    if let Err(err) = copy(from, to) {
        return error!("copy({from:#?}, {to:#?}) failed: '{err}'");
    }
    Ok(())
}

pub fn write_file(file_path: &Path, content: &str) -> Result<(), String> {
    if let Err(err) = write(file_path, content) {
        return error!("write({file_path:#?}) failed: '{err}'");
    }
    Ok(())
}

pub fn read_file(file_path: &Path) -> Result<String, String> {
    match read_to_string(file_path) {
        Ok(content) => Ok(content),
        Err(err) => error!("read_to_string({file_path:#?}) failed: '{err}'"),
    }
}

pub fn ls_regex(regex: &Path) -> Result<Vec<PathBuf>, String> {
    let Some(parent) = regex.parent() else {
        return error!("invalid regex {regex:#?}");
    };
    let Ok(entries) = read_dir(parent) else {
        return Ok(Vec::new());
    };
    Ok(entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (regex == wildcardize_path(&path)).then_some(path)
        })
        .collect())
}

pub fn ls_dir(path: &Path) -> Result<Vec<PathBuf>, String> {
    let Ok(entries) = read_dir(path) else {
        return Ok(Vec::new());
    };
    Ok(entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.is_dir().then_some(path)
        })
        .collect())
}
