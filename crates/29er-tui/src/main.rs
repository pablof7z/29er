//! Placeholder entrypoint. The real async runtime loop is wired in wave 4.

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    println!("29er-tui starting");
    Ok(())
}
