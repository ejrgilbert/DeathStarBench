fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile the cache gRPC contract into host-lib so it owns a concrete
    // `CacheClient`. This lets `SvcHostData` carry an `Option<CacheClient>`
    // without a second generic parameter, and lets `impl_cache_svc_grpc!`
    // reference the request/response types via `$crate::cache_proto`.
    let proto = "../proto/cache.proto";
    println!("cargo:rerun-if-changed={proto}");
    println!("cargo:rerun-if-changed=../proto");
    tonic_build::configure().compile_protos(&[proto], &["../proto"])?;
    Ok(())
}
