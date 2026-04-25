use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs");
    println!("cargo:rerun-if-changed=../Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");

    let base = env!("CARGO_PKG_VERSION");
    let major_minor = base.rsplit_once('.').map(|x| x.0).unwrap_or(base);
    let patch = compute_patch().unwrap_or_else(|| "0".to_string());

    println!("cargo:rustc-env=AIL_VERSION={major_minor}.{patch}");
}

fn compute_patch() -> Option<String> {
    let workspace_root = "..";

    let cargo_toml = std::fs::read_to_string(format!("{workspace_root}/Cargo.toml")).ok()?;
    let version_line = cargo_toml
        .lines()
        .find(|l| l.starts_with("version = \""))?
        .to_string();

    let anchor_out = Command::new("git")
        .current_dir(workspace_root)
        .args([
            "log",
            "--format=%H",
            "-S",
            &version_line,
            "--",
            "Cargo.toml",
        ])
        .output()
        .ok()?;
    let anchor = String::from_utf8(anchor_out.stdout)
        .ok()?
        .lines()
        .last()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let scope: &[&str] = &[
        "ail",
        "ail-core",
        "ail-init",
        "ail-spec",
        "Cargo.toml",
        "Cargo.lock",
    ];

    let mut args: Vec<String> = vec!["rev-list".into(), "--count".into()];
    args.push(match &anchor {
        Some(a) => format!("{a}..HEAD"),
        None => "HEAD".into(),
    });
    args.push("--".into());
    for s in scope {
        args.push((*s).to_string());
    }

    let count_out = Command::new("git")
        .current_dir(workspace_root)
        .args(&args)
        .output()
        .ok()?;
    let count = String::from_utf8(count_out.stdout)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())?;

    Some(count)
}
