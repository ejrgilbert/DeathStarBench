mod cache;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cache::run().await
}
