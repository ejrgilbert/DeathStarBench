fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = std::path::Path::new("../proto");
    tonic_build::configure()
        .compile_protos(
            &[
                proto_dir.join("recommendation.proto"),
                proto_dir.join("recommendation-store.proto"),
            ],
            &[proto_dir],
        )?;
    Ok(())
}
