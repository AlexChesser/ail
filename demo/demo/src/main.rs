use ail::ai_with_input;

fn main() {
    println!("AI is working!");
    let result = ai_with_input("hello world");
    println!("Input: \"hello world\"");
    println!("Output: {}", result);
}
