fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = std::path::Path::new("../../proto");
    println!("cargo:rerun-if-changed={}", proto_dir.display());

    let protos: Vec<_> = std::fs::read_dir(proto_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("proto"))
        .collect();

    tonic_build::configure().compile_protos(&protos, &[proto_dir])?;

    Ok(())
}
