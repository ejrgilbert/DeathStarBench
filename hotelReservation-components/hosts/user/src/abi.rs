use anyhow::Result;
use wasmtime::Store;
use host_lib::StoreData;
use crate::grpc::UserComponent;

wasmtime::component::bindgen!({
    path: "../../components/user/wit",
    world: "user-composed-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl UserComponent for UserComposedHostWorld {
    type Data = StoreData;

    async fn check_user(
        &self,
        store: &mut Store<Self::Data>,
        username: String,
        password: String,
    ) -> Result<bool> {
        Ok(self.hotel_api_user()
            .call_check_user(store, &username, &password).await?)
    }
}

host_lib::run_abi!(
    UserComposedHostWorld, hotel_api_user,
    "user-db",
    "0.0.0.0:8091", "user-composed.wasm", "user-host"
);
