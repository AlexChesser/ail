//! CI invariant: every pipeline YAML shipped inside a bundled template must
//! pass `ail_core::config::load` on its own. Schema drift in `ail-core` that
//! isn't reflected in the demos breaks this test before the release ships.

use ail_init::{run_in_cwd, InitArgs};
use std::path::{Path, PathBuf};

const TEMPLATES: &[&str] = &["starter", "superpowers", "oh-my-ail"];

#[test]
fn all_bundled_templates_validate() {
    let mut aggregate_failures: Vec<String> = Vec::new();

    for name in TEMPLATES {
        let tmp = tempfile::tempdir().expect("tempdir");
        run_in_cwd(
            InitArgs {
                template: Some((*name).to_string()),
                force: false,
                dry_run: false,
            },
            tmp.path(),
        )
        .unwrap_or_else(|e| panic!("install of `{name}` failed: {e}"));

        let install_root = tmp.path().join(".ail");
        let mut pipelines = Vec::new();
        collect_pipelines(&install_root, &mut pipelines);
        assert!(
            !pipelines.is_empty(),
            "template `{name}` produced no pipeline files to validate — install logic bug?"
        );

        for path in pipelines {
            if let Err(e) = ail_core::config::load(&path) {
                let rel = path
                    .strip_prefix(&install_root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                aggregate_failures.push(format!("[{name}] {rel}: {e}"));
            }
        }
    }

    assert!(
        aggregate_failures.is_empty(),
        "bundled template validation failed:\n  {}",
        aggregate_failures.join("\n  ")
    );
}

fn collect_pipelines(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_pipelines(&path, out);
        } else if is_pipeline_file(&path) {
            out.push(path);
        }
    }
}

fn is_pipeline_file(p: &Path) -> bool {
    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name == "default.yaml" || name.ends_with(".ail.yaml")
}
