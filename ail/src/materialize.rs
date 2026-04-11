pub fn handle_materialize(p: ail_core::config::domain::Pipeline, out: Option<std::path::PathBuf>) {
    let output = ail_core::materialize::materialize(&p);
    match out {
        Some(out_path) => {
            if let Err(e) = std::fs::write(&out_path, &output) {
                eprintln!("Failed to write to {}: {e}", out_path.display());
                std::process::exit(1);
            }
        }
        None => print!("{output}"),
    }
}
