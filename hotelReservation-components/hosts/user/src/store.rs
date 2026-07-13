use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("user_store");
}
use proto::{
    user_store_server::{UserStore, UserStoreServer},
    User as ProtoUser,
    InitRequest, InitResponse, LoadUsersRequest, LoadUsersResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/user_store/wit",
    world: "user-store-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(UserStoreHostWorld);

#[tonic::async_trait]
impl UserStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_user_data_user_store()
            .call_init(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_users(
        &self,
        _req: Request<LoadUsersRequest>,
    ) -> Result<Response<LoadUsersResponse>, Status> {
        let mut store = self.store.lock().await;
        let wit_users = self.instance
            .hotel_user_data_user_store()
            .call_load_users(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadUsersResponse {
            users: wit_users.into_iter().map(|u| ProtoUser {
                username: u.username,
                password: u.password,
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    UserStoreHostWorld, UserStoreServer, hotel_user_data_user_store,
    "user-db", "user",
    "0.0.0.0:8092", "user-store.wasm", "user-host"
);
