mod grpc;
mod svc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    svc::run().await
}
