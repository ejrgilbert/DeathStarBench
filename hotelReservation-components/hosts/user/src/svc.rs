use anyhow::Result;
use crate::grpc::UserComponent;

mod store_proto {
    tonic::include_proto!("user_store");
}
use store_proto::{user_store_client::UserStoreClient, LoadUsersRequest};

wasmtime::component::bindgen!({
    path: "../../components/user/wit",
    world: "user-host-world",
    imports: { default: async },
    exports: { default: async },
});

host_lib::svc_host_data!(UserStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

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

type UserSvcHost = host_lib::SvcHost<UserHostWorldPre<HostData>, UserStoreClient<tonic::transport::Channel>>;

#[async_trait::async_trait]
impl UserComponent for UserSvcHost {
    async fn check_user(&self, username: String, password: String) -> Result<bool> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_user().call_check_user(&mut store, &username, &password).await?)
    }
}

host_lib::run_svc_pre!(
    UserHostWorld, UserHostWorldPre<HostData>, UserStoreClient<tonic::transport::Channel>,
    "http://localhost:8092", "0.0.0.0:8091", "user.wasm", "user-host [svc]"
);
