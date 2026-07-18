use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use crate::grpc::UserComponent;

mod store_proto {
    tonic::include_proto!("user_store");
}
use store_proto::{user_store_client::UserStoreClient, LoadUsersRequest};

wasmtime::component::bindgen!({
    path: "../../components/user/wit",
    world: "user-host-world",
    async: true,
});

host_lib::svc_host_data!(UserStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::user_store::Host for HostData {
    async fn load_users(&mut self) -> Vec<hotel::store::user_store::User> {
        let resp = self.store_client.lock().await
            .load_users(tonic::Request::new(LoadUsersRequest {})).await
            .expect("gRPC user-store LoadUsers failed").into_inner();
        resp.users.into_iter().map(|u| hotel::store::user_store::User {
            username: u.username,
            password: u.password,
        }).collect()
    }
}

struct UserSvcHost {
    engine:       Arc<wasmtime::Engine>,
    pre:          Arc<UserHostWorldPre<HostData>>,
    store_client: Arc<Mutex<UserStoreClient<tonic::transport::Channel>>>,
    cache:        Arc<host_lib::Cache>,
}

#[async_trait::async_trait]
impl UserComponent for UserSvcHost {
    async fn check_user(&self, username: String, password: String) -> Result<bool> {
        let data = HostData {
            wasi:         host_lib::make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_user().call_check_user(&mut store, &username, &password).await?)
    }
}

pub async fn run() -> Result<()> {
    let store_addr  = std::env::var("STORE_ADDR").unwrap_or("http://localhost:8092".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8091".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("user.wasm".into());

    let store_client = UserStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache        = Arc::new(host_lib::Cache::new(Default::default()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    UserHostWorld::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(UserHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("user-host [svc] listening on {listen_addr}");
    crate::grpc::serve(Arc::new(UserSvcHost { engine, pre, store_client, cache }), listen_addr).await
}
