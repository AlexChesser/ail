use clap::Parser;

#[derive(Parser)]
#[command(name = "ail", version = ail_core::version(), about = "Artificial Intelligence Loops — the control plane for how agents behave after the human stops typing.")]
struct Cli {}

fn main() {
    tracing_subscriber::fmt().json().init();

    let _cli = Cli::parse();

    tracing::info!(event = "startup", version = ail_core::version());

    println!("ail {}", ail_core::version());
}
