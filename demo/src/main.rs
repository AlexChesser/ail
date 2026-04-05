// AIL CLI demo - demonstrates --once flow
use ail::ai_with_input;

fn main() {
    println!("=== Demonstrating --once Flow ===\n");

    // Command 1: Run the pipeline --once with the step "add fizzbuzz function"
    // This demonstrates:
    // 1. --once flows to the execution
    // 2. Executes one step
    // 3. Outputs the result
    let stdout = ai_with_input("add fizzbuzz function");
    println!("Output from step: {}", stdout);

    // Command 2: Display direct output
    println!("\n=== Direct Output ===\n");
    println!("This output is from running 'ail --once fizzbuzz function' directly\n");
    println!("This outputs:\n");
    println!("add fizzbuzz function");

    // Show result directly
    println!("\nResult of 'add fizzbuzz function': {}", ai_with_input("add fizzbuzz function"));
}
