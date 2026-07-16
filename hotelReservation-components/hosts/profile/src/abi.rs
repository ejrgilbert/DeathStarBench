use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::{Component, Linker};
use crate::grpc::{ProfileComponent, Hotel, Address, Image};

wasmtime::component::bindgen!({
    path: "../../components/profile/wit",
    world: "profile-composed-host-world",
    async: true,
});

pub struct AbiData {
    pub wasi:       wasmtime_wasi::WasiCtx,
    pub table:      wasmtime_wasi::ResourceTable,
    pub collection: Arc<mongodb::Collection<bson::Document>>,
}

impl wasmtime_wasi::WasiView for AbiData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

host_lib::impl_collection_host!(AbiData);

#[async_trait::async_trait]
impl ProfileComponent for ProfileComposedHostWorld {
    type Data = AbiData;

    async fn get_profiles(
        &self,
        store: &mut Store<AbiData>,
        hotel_ids: Vec<String>,
    ) -> Result<Vec<Hotel>> {
        let wit_hotels = self.hotel_profile_profile()
            .call_get_profiles(store, &hotel_ids).await?;
        Ok(wit_hotels.into_iter().map(|p| Hotel {
            id:           p.id,
            name:         p.name,
            phone_number: p.phone_number,
            description:  p.description,
            address: Some(Address {
                street_number: p.addr.street_number,
                street_name:   p.addr.street_name,
                city:          p.addr.city,
                state:         p.addr.state,
                country:       p.addr.country,
                postal_code:   p.addr.postal_code,
                lat:           p.addr.lat as f32,
                lon:           p.addr.lon as f32,
            }),
            images: p.images.into_iter().map(|img| Image {
                url:     img.url,
                default: img.default,
            }).collect(),
        }).collect())
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8095".into())
        .parse()?;
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "profile-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(mongo.database("profile-db").collection("inventory"));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<AbiData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ProfileComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = AbiData {
        wasi:       host_lib::make_store_wasi_ctx(&data_dir)?,
        table:      wasmtime_wasi::ResourceTable::new(),
        collection,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance = ProfileComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_profile_profile().call_init(&mut store).await?;

    println!("profile-host [abi] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
