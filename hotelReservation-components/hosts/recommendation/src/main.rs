mod grpc;
mod store;
mod svc;
mod abi;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mode = std::env::var("MODE").unwrap_or_else(|_| "abi".into());
    match mode.as_str() {
        "store" => store::run().await,
        "svc"   => svc::run().await,
        "abi"   => abi::run().await,
        other   => anyhow::bail!("unknown MODE={other}; expected store|svc|abi"),
    }
}
