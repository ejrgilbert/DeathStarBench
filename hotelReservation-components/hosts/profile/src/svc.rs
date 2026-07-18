use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use crate::grpc::{ProfileComponent, Hotel, Address, Image};

mod store_proto {
    tonic::include_proto!("profile_store");
}
use store_proto::{profile_store_client::ProfileStoreClient, LoadProfilesRequest};

wasmtime::component::bindgen!({
    path: "../../components/profile/wit",
    world: "profile-host-world",
    async: true,
});

pub struct HostData {
    pub wasi:         wasmtime_wasi::WasiCtx,
    pub table:        wasmtime_wasi::ResourceTable,
    pub store_client: Arc<Mutex<ProfileStoreClient<tonic::transport::Channel>>>,
    pub cache:        Arc<host_lib::Cache>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::profile_store::Host for HostData {
    async fn load_profiles(&mut self) -> Vec<hotel::store::profile_store::Hotel> {
        let resp = self.store_client.lock().await
            .load_profiles(tonic::Request::new(LoadProfilesRequest {})).await
            .expect("gRPC profile-store LoadProfiles failed").into_inner();
        resp.profs.into_iter().map(|h| {
            let a = h.address.unwrap_or_default();
            hotel::store::profile_store::Hotel {
                id:           h.id,
                name:         h.name,
                phone_number: h.phone_number,
                description:  h.description,
                addr: hotel::store::profile_store::Address {
                    street_number: a.street_number,
                    street_name:   a.street_name,
                    city:          a.city,
                    state:         a.state,
                    country:       a.country,
                    postal_code:   a.postal_code,
                    lat:           a.lat as f64,
                    lon:           a.lon as f64,
                },
                images: h.images.into_iter().map(|img| hotel::store::profile_store::Image {
                    url:     img.url,
                    default: img.default,
                }).collect(),
            }
        }).collect()
    }
}

struct ProfileSvcHost {
    engine:       Arc<wasmtime::Engine>,
    pre:          Arc<ProfileHostWorldPre<HostData>>,
    store_client: Arc<Mutex<ProfileStoreClient<tonic::transport::Channel>>>,
    cache:        Arc<host_lib::Cache>,
}

#[async_trait::async_trait]
impl ProfileComponent for ProfileSvcHost {
    async fn get_profiles(&self, hotel_ids: Vec<String>) -> Result<Vec<Hotel>> {
        let data = HostData {
            wasi:         host_lib::make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
        let wit_hotels = instance.hotel_api_profile()
            .call_get_profiles(&mut store, &hotel_ids).await?;
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
    let store_addr = std::env::var("STORE_ADDR").unwrap_or("http://localhost:8096".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8095".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("profile.wasm".into());

    let store_client = ProfileStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache        = Arc::new(host_lib::Cache::new(Default::default()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ProfileHostWorld::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(ProfileHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("profile-host [svc] listening on {listen_addr}");
    crate::grpc::serve(Arc::new(ProfileSvcHost { engine, pre, store_client, cache }), listen_addr).await
}
