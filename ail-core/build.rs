fn main() -> Result<(), Box<dyn std::error::Error>> {
    vergen::EmitBuilder::builder()
        .build_date()
        .git_sha(true)
        .emit()?;
    Ok(())
}
