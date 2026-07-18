mod grpc;
mod store;
mod svc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mode = std::env::var("MODE").unwrap_or_else(|_| "svc".into());
    match mode.as_str() {
        "store" => store::run().await,
        "svc"   => svc::run().await,
        other   => anyhow::bail!("unknown MODE={other}; expected store|svc"),
    }
}
