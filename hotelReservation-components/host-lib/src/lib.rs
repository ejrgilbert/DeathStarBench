use anyhow::Result;
use std::sync::Arc;
use wasmtime::{Config, Engine};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder};

// Proto-generated types, re-exported for use by host binaries.
pub mod recommendation {
    tonic::include_proto!("recommendation");
}
pub mod recommendation_store {
    tonic::include_proto!("recommendation_store");
}

// Bindings for the recommendation-store component host world.
// Defines RecommendationStoreHostWorld, host::storage::collection::Host, and StoreData.
wasmtime::component::bindgen!({
    path: "../components/recommendation_store/wit",
    world: "recommendation-store-host-world",
    async: true,
});

/// Store data for the recommendation-store host.
/// Owns both the WASI context and the MongoDB collection handle.
pub struct StoreData {
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub collection: Arc<mongodb::Collection<bson::Document>>,
}

impl wasmtime_wasi::WasiView for StoreData {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl host::storage::collection::Host for StoreData {
    async fn count(&mut self) -> u64 {
        mongo_count(&self.collection).await.unwrap_or(0)
    }

    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        mongo_find_all(&self.collection).await.unwrap_or_default()
    }

    async fn find_one(&mut self, _filter: Vec<u8>) -> Option<Vec<u8>> {
        unimplemented!("find_one not used by recommendation-store")
    }

    async fn find(&mut self, _filter: Vec<u8>) -> Vec<Vec<u8>> {
        unimplemented!("find not used by recommendation-store")
    }

    async fn insert_one(&mut self, _doc: Vec<u8>) {
        unimplemented!("insert_one not used by recommendation-store")
    }

    async fn insert_many(&mut self, docs: Vec<Vec<u8>>) {
        mongo_insert_many(&self.collection, docs).await.unwrap();
    }
}

pub fn make_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
    Ok(Engine::new(&config)?)
}

pub fn make_store_wasi_ctx(data_dir: &str) -> Result<WasiCtx> {
    Ok(WasiCtxBuilder::new()
        .inherit_env()
        .inherit_stdout()
        .inherit_stderr()
        .preopened_dir(data_dir, "/data", DirPerms::READ, FilePerms::READ)?
        .build())
}

pub fn make_wasi_ctx() -> WasiCtx {
    WasiCtxBuilder::new()
        .inherit_env()
        .inherit_stdout()
        .inherit_stderr()
        .build()
}

fn bson_to_json(doc: &bson::Document) -> Result<Vec<u8>> {
    let val = bson::from_document::<serde_json::Value>(doc.clone())?;
    Ok(serde_json::to_vec(&val)?)
}

fn json_to_bson(bytes: &[u8]) -> Result<bson::Document> {
    let val: serde_json::Value = serde_json::from_slice(bytes)?;
    Ok(bson::to_document(&val)?)
}

async fn mongo_find_all(
    collection: &Arc<mongodb::Collection<bson::Document>>,
) -> Result<Vec<Vec<u8>>> {
    use futures::TryStreamExt;
    let mut cursor = collection.find(bson::doc! {}).await?;
    let mut out = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        out.push(bson_to_json(&doc)?);
    }
    Ok(out)
}

async fn mongo_count(collection: &Arc<mongodb::Collection<bson::Document>>) -> Result<u64> {
    Ok(collection.count_documents(bson::doc! {}).await?)
}

async fn mongo_insert_many(
    collection: &Arc<mongodb::Collection<bson::Document>>,
    docs: Vec<Vec<u8>>,
) -> Result<()> {
    let bson_docs: Vec<bson::Document> =
        docs.iter().map(|b| json_to_bson(b)).collect::<Result<_>>()?;
    collection.insert_many(bson_docs).await?;
    Ok(())
}
