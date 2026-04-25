pub fn print_landing_page() {
    println!(
        "ail — Artificial Intelligence Loops {}",
        ail_core::version()
    );
    println!();
    println!("The control plane for how agents behave after the human stops typing.");
    println!();
    println!("Workflow:");
    println!("  ail init                    Scaffold an ail workspace from a template");
    println!("  ail \"<prompt>\"              Run a prompt through the discovered pipeline");
    println!();
    println!("Inspection:");
    println!("  ail validate                Check a pipeline file for errors without running it");
    println!("  ail materialize             Print the fully-resolved pipeline YAML");
    println!("  ail logs                    Query recorded pipeline runs");
    println!();
    println!("Reference:");
    println!("  ail spec                    Browse the embedded spec");
    println!();
    println!("Key flags:");
    println!("  --pipeline <path>           Override pipeline file discovery");
    println!(
        "  --dry-run                   Resolve and print the pipeline without making LLM calls"
    );
    println!("  --headless                  Skip all tool-use permission prompts");
    println!();
    println!("Examples:");
    println!("  ail \"refactor the auth module\"");
    println!("  ail \"fix failing tests\" --pipeline .ail/strict.yaml");
    println!("  ail init starter");
    println!("  ail spec --list");
    println!("  ail validate --pipeline .ail.yaml");
    println!();
    println!("Run `ail --help` for all flags  ·  `ail <command> --help` for command options.");
}
