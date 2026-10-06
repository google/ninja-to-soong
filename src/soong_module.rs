// Copyright 2024 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::*;

pub enum CcLibraryHeaders {
    SpirvTools,
    SpirvHeaders,
    SpirvHeadersUnified1,
    Llvm,
    Clang,
}
impl CcLibraryHeaders {
    pub fn str(self) -> String {
        String::from(match self {
            Self::SpirvTools => "SPIRV-Tools-includes",
            Self::SpirvHeaders => "SPIRV-Headers-includes",
            Self::SpirvHeadersUnified1 => "SPIRV-Headers-includes-unified1",
            Self::Llvm => "llvm-includes",
            Self::Clang => "clang-includes",
        })
    }
}

pub enum CcDefaults {
    ClspvLlvmDependencies,
    Llvm,
    OpenclCts,
    OpenclCtsManual,
    Angle,
    AngleVendor,
    MediaDriver,
    Mesa3DIntel,
    Mesa3DIntelManual,
    Mesa3DPanvk,
    Mesa3DPanvkManual,
}
impl CcDefaults {
    pub fn str(self) -> String {
        String::from(match self {
            Self::ClspvLlvmDependencies => "clspv-llvm-dependencies",
            Self::Llvm => "llvm-project-defaults",
            Self::OpenclCts => "OpenCL-CTS-defaults",
            Self::OpenclCtsManual => "OpenCL-CTS-manual-defaults",
            Self::Angle => "angle-common-defaults",
            Self::AngleVendor => "angle_vendor_cc_defaults",
            Self::MediaDriver => "media-driver-defaults",
            Self::Mesa3DIntel => "desktop-mesa3d-intel-defaults",
            Self::Mesa3DIntelManual => "desktop-mesa3d-intel-raw-defaults",
            Self::Mesa3DPanvk => "desktop-mesa3d-panvk-defaults",
            Self::Mesa3DPanvkManual => "desktop-mesa3d-panvk-raw-defaults",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SoongProp {
    Str(String),
    VecStr(Vec<String>),
    Bool(bool),
    Prop(Vec<SoongNamedProp>),
    None,
}

impl From<bool> for SoongProp {
    fn from(bool: bool) -> Self {
        Self::Bool(bool)
    }
}

impl From<&str> for SoongProp {
    fn from(str: &str) -> Self {
        Self::Str(String::from(str))
    }
}

impl From<String> for SoongProp {
    fn from(str: String) -> Self {
        Self::Str(str)
    }
}

impl From<Vec<String>> for SoongProp {
    fn from(vec_str: Vec<String>) -> Self {
        Self::VecStr(vec_str)
    }
}

impl From<Vec<&str>> for SoongProp {
    fn from(vec_str: Vec<&str>) -> Self {
        Self::VecStr(vec_str.into_iter().map(String::from).collect())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoongNamedProp {
    name: String,
    prop: SoongProp,
    wildcard_src_path: Option<PathBuf>,
}

impl SoongNamedProp {
    pub fn new(name: &str, prop: impl Into<SoongProp>) -> Self {
        Self {
            name: String::from(name),
            prop: prop.into(),
            wildcard_src_path: None,
        }
    }

    pub fn enable_wildcard(&mut self, src_path: &Path) -> Result<(), String> {
        let SoongProp::VecStr(_) = &self.prop else {
            return error!("Could not wildcardize a non-VecStr property");
        };
        self.wildcard_src_path = Some(PathBuf::from(src_path));
        Ok(())
    }

    pub fn get_prop(&self) -> SoongProp {
        self.prop.clone()
    }

    pub fn filter_default(
        &mut self,
        default_prop: SoongProp,
        base_name: &str,
    ) -> Result<(), String> {
        match default_prop {
            SoongProp::VecStr(default_vec_str) => match &mut self.prop {
                SoongProp::VecStr(vec_str) => {
                    if self.name != "defaults" {
                        for str in &default_vec_str {
                            if !vec_str.contains(str) {
                                return error!("Could not filter {0:#?} from {base_name:#?} because it does not contain {str:#?}", self.name);
                            }
                        }
                    }
                    vec_str.retain(|str| !default_vec_str.contains(str));
                }
                _ => return error!("default prop type (VecStr) does not match with named prop"),
            },
            SoongProp::Str(default_str) => match &self.prop {
                SoongProp::Str(str) => {
                    if &default_str != str {
                        return error!("Could not filter {0:#?} from {base_name:#?} because it is different than default ({default_str:#?} != {str:#?})", self.name);
                    }
                    self.prop = SoongProp::None;
                }
                _ => return error!("default prop type (Str) does not match with named prop"),
            },
            SoongProp::Prop(default_props) => match &mut self.prop {
                SoongProp::Prop(props) => {
                    for default_prop in &default_props {
                        let Some(prop) =
                            props.iter_mut().find(|prop| prop.name == default_prop.name)
                        else {
                            return error!("Could not filter {0:#?} from {base_name:#?} because default prop {1:#?} could not be found", self.name, default_prop.name);
                        };
                        prop.filter_default(default_prop.get_prop(), base_name)?;
                    }
                }
                _ => return error!("default prop type (Prop) does not match with named prop"),
            },
            SoongProp::Bool(default_bool) => match &self.prop {
                SoongProp::Bool(bool) => {
                    if &default_bool != bool {
                        return error!("Could not filter {0:#?} from {base_name:#?} because it is different than default ({default_bool:#?} != {bool:#?})", self.name);
                    }
                    self.prop = SoongProp::None;
                }
                _ => return error!("default prop type (Bool) does not match with named prop"),
            },
            _ => return error!("Unsupported property type to filter"),
        }
        Ok(())
    }

    fn print(self, indent_level: usize) -> String {
        const INDENT: &str = "    ";
        let indent = INDENT.repeat(indent_level);
        let content = match self.prop {
            SoongProp::None => String::new(),
            SoongProp::Str(str) => format!("\"{str}\""),
            SoongProp::Bool(bool) => format!("{bool}"),
            SoongProp::Prop(props) => {
                let content = props
                    .into_iter()
                    .map(|prop| prop.print(indent_level + 1))
                    .collect::<Vec<String>>()
                    .concat();
                if content.is_empty() {
                    String::new()
                } else {
                    format!("{{\n{content}{indent}}}")
                }
            }
            SoongProp::VecStr(mut vec_str) => {
                if vec_str.is_empty() {
                    return String::new();
                }
                if let Some(src_path) = self.wildcard_src_path {
                    vec_str = wildcardize_paths(vec_str, &src_path);
                }
                vec_str.sort_unstable();
                vec_str.dedup();
                if vec_str.len() == 1 {
                    format!("[\"{0}\"]", vec_str[0])
                } else {
                    let indent_next = INDENT.repeat(indent_level + 1);
                    format!(
                        "[\n{0}{indent}]",
                        vec_str
                            .iter()
                            .map(|str| format!("{indent_next}\"{str}\",\n",))
                            .collect::<Vec<String>>()
                            .concat()
                    )
                }
            }
        };
        if content.is_empty() {
            String::new()
        } else {
            format!("{indent}{0}: {content},\n", self.name)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoongModule {
    name: String,
    props: Vec<SoongNamedProp>,
}

impl SoongModule {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            props: Vec::new(),
        }
    }

    pub fn new_cc_defaults(name: CcDefaults) -> Self {
        Self::new("cc_defaults").add_prop("name", name.str())
    }

    pub fn new_cc_library_headers(name: CcLibraryHeaders, include_dirs: Vec<String>) -> Self {
        Self::new("cc_library_headers")
            .add_prop("name", name.str())
            .add_prop("export_include_dirs", include_dirs)
            .add_prop("vendor_available", true)
            .add_prop("host_supported", true)
    }

    pub fn new_filegroup(name: String, files: Vec<String>) -> Self {
        Self::new("filegroup")
            .add_prop("name", name)
            .add_prop("srcs", files)
    }

    pub fn extend_prop(mut self, name: &str, vec_str: Vec<&str>) -> Result<SoongModule, String> {
        let merge_prop = |prop: SoongProp| {
            let SoongProp::VecStr(mut new_vec_str) = prop else {
                return error!(
                    "Cannot extend {name}, only VecStr can be extended through 'extend_prop'"
                );
            };
            new_vec_str.extend(vec_str.iter().map(|str| String::from(*str)));
            Ok(SoongProp::VecStr(new_vec_str))
        };
        if !self.update_prop(name, merge_prop)? {
            self.props.push(SoongNamedProp::new(name, vec_str));
        }
        Ok(self)
    }

    pub fn add_defaults(self, default: CcDefaults) -> Result<SoongModule, String> {
        self.extend_prop("defaults", vec![&default.str()])
    }

    pub fn add_named_prop(mut self, prop: SoongNamedProp) -> SoongModule {
        self.props.push(prop);
        self
    }

    pub fn add_prop(mut self, name: &str, prop: impl Into<SoongProp>) -> SoongModule {
        self.props.push(SoongNamedProp::new(name, prop));
        self
    }

    pub fn add_props(mut self, props: Vec<SoongNamedProp>) -> SoongModule {
        self.props.extend(props);
        self
    }

    pub fn update_prop<F>(&mut self, name: &str, f: F) -> Result<bool, String>
    where
        F: Fn(SoongProp) -> Result<SoongProp, String>,
    {
        let Some(named_prop) = self.props.iter_mut().find(|prop| prop.name == name) else {
            return Ok(false);
        };
        named_prop.prop = f(std::mem::replace(&mut named_prop.prop, SoongProp::None))?;
        Ok(true)
    }

    pub fn filter_default(&mut self, default: &SoongModule) -> Result<(), String> {
        let Some(SoongNamedProp {
            prop: SoongProp::Str(my_name),
            ..
        }) = self.get_prop("name")
        else {
            return error!("No 'name' property in {self:#?}");
        };
        for default_prop in &default.props {
            let name = &default_prop.name;
            if name == "name" {
                continue;
            }
            let Some(self_prop) = self.props.iter_mut().find(|prop| &prop.name == name) else {
                return error!("Could not find prop '{name}' in module properties:\n{self:#?}");
            };
            self_prop.filter_default(default_prop.get_prop(), &my_name)?;
        }
        Ok(())
    }

    pub fn get_prop(&self, name: &str) -> Option<SoongNamedProp> {
        self.props.iter().find(|prop| prop.name == name).cloned()
    }

    pub fn pop_prop(&mut self, name: &str) -> Option<SoongNamedProp> {
        let prop_idx = self.props.iter().position(|prop| prop.name == name)?;
        Some(self.props.remove(prop_idx))
    }

    pub fn get_props_name(&self) -> Vec<String> {
        self.props.iter().map(|prop| prop.name.clone()).collect()
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn print(self) -> String {
        format!(
            "\n{0} {{\n{1}}}\n",
            self.name,
            self.props
                .into_iter()
                .map(|prop| prop.print(1))
                .collect::<Vec<String>>()
                .concat()
        )
    }
}
