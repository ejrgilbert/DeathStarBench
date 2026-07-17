use anyhow::Result;
use wasmtime::Store;
use crate::grpc::UserComponent;

mod store_proto {
    tonic::include_proto!("user_store");
}
use store_proto::{user_store_client::UserStoreClient, InitRequest, LoadUsersRequest};

wasmtime::component::bindgen!({
    path: "../../components/user/wit",
    world: "user-host-world",
    async: true,
});

host_lib::svc_host_data!(UserStoreClient<tonic::transport::Channel>);

#[async_trait::async_trait]
impl hotel::store::user_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC user-store Init failed");
    }

    async fn load_users(&mut self) -> Vec<hotel::store::user_store::User> {
        let resp = self.store_client.lock().await
            .load_users(tonic::Request::new(LoadUsersRequest {})).await
            .expect("gRPC user-store LoadUsers failed")
            .into_inner();
        resp.users.into_iter().map(|u| hotel::store::user_store::User {
            username: u.username,
            password: u.password,
        }).collect()
    }
}

#[async_trait::async_trait]
impl UserComponent for UserHostWorld {
    type Data = HostData;

    async fn check_user(
        &self,
        store: &mut Store<HostData>,
        username: String,
        password: String,
    ) -> Result<bool> {
        Ok(self.hotel_api_user()
            .call_check_user(store, &username, &password).await?)
    }
}

host_lib::run_svc!(
    UserHostWorld,
    UserStoreClient<tonic::transport::Channel>,
    hotel_api_user,
    "http://localhost:8092",
    "0.0.0.0:8091",
    "user.wasm",
    "user-host"
);
