use anyhow::Result;
use wasmtime::Store;
use host_lib::StoreData;
use crate::grpc::AttractionsComponent;

wasmtime::component::bindgen!({
    path: "../../components/attractions/wit",
    world: "attractions-composed-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl AttractionsComponent for AttractionsComposedHostWorld {
    type Data = StoreData;

    async fn nearby_rest(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_attractions()
            .call_nearby_rest(store, &hotel_id).await?)
    }

    async fn nearby_mus(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_attractions()
            .call_nearby_mus(store, &hotel_id).await?)
    }

    async fn nearby_cinema(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_attractions()
            .call_nearby_cinema(store, &hotel_id).await?)
    }
}

host_lib::run_abi!(
    AttractionsComposedHostWorld, hotel_api_attractions,
    "attractions-db", "attractions",
    "0.0.0.0:8087", "attractions-composed.wasm", "attractions-host"
);
