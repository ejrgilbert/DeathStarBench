use anyhow::Result;
use std::sync::Arc;
use wasmtime::{Config, Engine};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder};

pub fn make_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
    Ok(Engine::new(&config)?)
}

pub fn make_wasi_ctx() -> WasiCtx {
    WasiCtxBuilder::new()
        .inherit_env()
        .inherit_stdout()
        .inherit_stderr()
        .build()
}

pub fn make_store_wasi_ctx(data_dir: &str) -> Result<WasiCtx> {
    Ok(WasiCtxBuilder::new()
        .inherit_env()
        .inherit_stdout()
        .inherit_stderr()
        .preopened_dir(data_dir, "/data", DirPerms::READ, FilePerms::READ)?
        .build())
}

/// Store data shared by all single-collection store hosts.
/// Each host's bindgen!-generated `host::storage::collection::Host` trait
/// is implemented on this type in the host binary itself (local trait, foreign type — allowed).
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

pub fn bson_to_json(doc: &bson::Document) -> Result<Vec<u8>> {
    let val = bson::from_document::<serde_json::Value>(doc.clone())?;
    Ok(serde_json::to_vec(&val)?)
}

pub fn json_to_bson(bytes: &[u8]) -> Result<bson::Document> {
    let val: serde_json::Value = serde_json::from_slice(bytes)?;
    Ok(bson::to_document(&val)?)
}

pub async fn mongo_find_all(
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

pub async fn mongo_count(collection: &Arc<mongodb::Collection<bson::Document>>) -> Result<u64> {
    Ok(collection.count_documents(bson::doc! {}).await?)
}

pub async fn mongo_insert_many(
    collection: &Arc<mongodb::Collection<bson::Document>>,
    docs: Vec<Vec<u8>>,
) -> Result<()> {
    let bson_docs: Vec<bson::Document> =
        docs.iter().map(|b| json_to_bson(b)).collect::<Result<_>>()?;
    collection.insert_many(bson_docs).await?;
    Ok(())
}

/// Implements `host::storage::collection::Host` on a type that has a `collection` field
/// of type `Arc<mongodb::Collection<bson::Document>>`.
///
/// The trait is generated locally by each host's `bindgen!` call, so the impl must
/// live in each host crate — this macro just eliminates the copy-paste of the body.
///
/// Usage: `host_lib::impl_collection_host!(YourStoreDataType);`
#[macro_export]
macro_rules! impl_collection_host {
    ($T:ty) => {
        #[::async_trait::async_trait]
        impl host::storage::collection::Host for $T {
            async fn count(&mut self) -> u64 {
                $crate::mongo_count(&self.collection).await.unwrap_or(0)
            }
            async fn find_all(&mut self) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                $crate::mongo_find_all(&self.collection).await.unwrap_or_default()
            }
            async fn find_one(&mut self, _: ::std::vec::Vec<u8>) -> ::core::option::Option<::std::vec::Vec<u8>> {
                unimplemented!()
            }
            async fn find(&mut self, _: ::std::vec::Vec<u8>) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                unimplemented!()
            }
            async fn insert_one(&mut self, _: ::std::vec::Vec<u8>) {
                unimplemented!()
            }
            async fn insert_many(&mut self, docs: ::std::vec::Vec<::std::vec::Vec<u8>>) {
                $crate::mongo_insert_many(&self.collection, docs).await.unwrap()
            }
        }
    };
}
