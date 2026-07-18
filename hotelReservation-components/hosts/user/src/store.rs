use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("user_store");
}
use proto::{
    user_store_server::{UserStore, UserStoreServer},
    User as ProtoUser,
    LoadUsersRequest, LoadUsersResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/user_store/wit",
    world: "user-store-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(UserStoreHostWorld, UserStoreHostWorldPre<host_lib::StoreData>);

#[tonic::async_trait]
impl UserStore for StoreGrpcService {
    async fn load_users(
        &self,
        _req: Request<LoadUsersRequest>,
    ) -> Result<Response<LoadUsersResponse>, Status> {
        let (mut store, instance) = self.new_instance().await
            .map_err(|e| Status::internal(e.to_string()))?;
        let wit_users = instance
            .hotel_store_user_store()
            .call_load_users(&mut store).await
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
    UserStoreHostWorld, UserStoreHostWorldPre<host_lib::StoreData>, UserStoreServer,
    "user-db",
    "0.0.0.0:8092", "user-store.wasm", "user-host"
);
