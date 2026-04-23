#![allow(clippy::result_large_err)]

use crate::template::Template;
use ail_core::error::AilError;
use std::path::{Path, PathBuf};

/// All templates install under `$CWD/.ail/` — matches the discovery rule for
/// `.ail/default.yaml` and keeps the user's CWD root uncluttered regardless
/// of how many files a template ships.
pub const INSTALL_SUBDIR: &str = ".ail";

pub struct InstallPlan {
    pub install_root: PathBuf,
    pub files: Vec<InstallFile>,
    pub conflicts: Vec<PathBuf>,
}

pub struct InstallFile {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub contents: Vec<u8>,
}

pub fn plan(template: &Template, cwd: &Path) -> InstallPlan {
    let install_root = cwd.join(INSTALL_SUBDIR);
    let mut files = Vec::with_capacity(template.files.len());
    let mut conflicts = Vec::new();

    for f in &template.files {
        let absolute_path = install_root.join(&f.relative_path);
        if absolute_path.exists() {
            conflicts.push(absolute_path.clone());
        }
        files.push(InstallFile {
            relative_path: f.relative_path.clone(),
            absolute_path,
            contents: f.contents.clone(),
        });
    }

    InstallPlan {
        install_root,
        files,
        conflicts,
    }
}

pub fn apply(plan: &InstallPlan, force: bool) -> Result<(), AilError> {
    if !plan.conflicts.is_empty() && !force {
        let list = plan
            .conflicts
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(AilError::config_validation(format!(
            "cannot install — {} file(s) already exist:\n{list}\n\nRe-run with --force to overwrite.",
            plan.conflicts.len()
        )));
    }

    for f in &plan.files {
        if let Some(parent) = f.absolute_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AilError::config_validation(format!(
                    "failed to create directory `{}`: {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(&f.absolute_path, &f.contents).map_err(|e| {
            AilError::config_validation(format!(
                "failed to write `{}`: {e}",
                f.absolute_path.display()
            ))
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{TemplateFile, TemplateMeta};

    fn sample_template() -> Template {
        Template {
            meta: TemplateMeta {
                name: "t".to_string(),
                aliases: vec![],
                short_description: "test".to_string(),
                tags: vec![],
            },
            files: vec![
                TemplateFile {
                    relative_path: PathBuf::from("a.yaml"),
                    contents: b"a\n".to_vec(),
                },
                TemplateFile {
                    relative_path: PathBuf::from("sub/b.yaml"),
                    contents: b"b\n".to_vec(),
                },
            ],
        }
    }

    #[test]
    fn plan_computes_install_root_and_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let p = plan(&sample_template(), tmp.path());
        assert_eq!(p.install_root, tmp.path().join(".ail"));
        assert_eq!(p.files.len(), 2);
        assert_eq!(p.files[0].absolute_path, tmp.path().join(".ail/a.yaml"));
        assert_eq!(p.files[1].absolute_path, tmp.path().join(".ail/sub/b.yaml"));
        assert!(p.conflicts.is_empty());
    }

    #[test]
    fn apply_writes_all_files_and_creates_subdirs() {
        let tmp = tempfile::tempdir().unwrap();
        let p = plan(&sample_template(), tmp.path());
        apply(&p, false).unwrap();
        assert_eq!(
            std::fs::read(tmp.path().join(".ail/a.yaml")).unwrap(),
            b"a\n"
        );
        assert_eq!(
            std::fs::read(tmp.path().join(".ail/sub/b.yaml")).unwrap(),
            b"b\n"
        );
    }

    #[test]
    fn conflicts_detected_when_target_exists() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".ail")).unwrap();
        std::fs::write(tmp.path().join(".ail/a.yaml"), b"existing").unwrap();

        let p = plan(&sample_template(), tmp.path());
        assert_eq!(p.conflicts.len(), 1);
        assert_eq!(p.conflicts[0], tmp.path().join(".ail/a.yaml"));
    }

    #[test]
    fn apply_refuses_without_force_and_preserves_existing_contents() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".ail")).unwrap();
        std::fs::write(tmp.path().join(".ail/a.yaml"), b"existing").unwrap();

        let p = plan(&sample_template(), tmp.path());
        let err = apply(&p, false).unwrap_err();
        assert!(err.detail().contains("already exist"));
        assert!(err.detail().contains("--force"));

        assert_eq!(
            std::fs::read(tmp.path().join(".ail/a.yaml")).unwrap(),
            b"existing"
        );
    }

    #[test]
    fn apply_overwrites_with_force() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".ail")).unwrap();
        std::fs::write(tmp.path().join(".ail/a.yaml"), b"existing").unwrap();

        let p = plan(&sample_template(), tmp.path());
        apply(&p, true).unwrap();

        assert_eq!(
            std::fs::read(tmp.path().join(".ail/a.yaml")).unwrap(),
            b"a\n"
        );
    }
}
