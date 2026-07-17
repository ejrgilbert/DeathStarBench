mod abi;
mod tcp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mode = std::env::var("MODE").unwrap_or_else(|_| "tcp".into());
    match mode.as_str() {
        "abi" => abi::run().await,
        _     => tcp::run().await,
    }
}
