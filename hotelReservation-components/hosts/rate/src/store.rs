use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("rate_store");
}
use proto::{
    rate_store_server::{RateStore, RateStoreServer},
    RatePlan as ProtoRatePlan, RoomType as ProtoRoomType,
    LoadRatesRequest, LoadRatesResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/rate_store/wit",
    world: "rate-store-host-world",
    imports: { default: async },
    exports: { default: async },
    with: {
        "host:storage/collection.connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(RateStoreHostWorld, RateStoreHostWorldPre<host_lib::StoreData>);

#[tonic::async_trait]
impl RateStore for StoreGrpcService {
    async fn load_rates(
        &self,
        _req: Request<LoadRatesRequest>,
    ) -> Result<Response<LoadRatesResponse>, Status> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let wit_rates = instance
            .hotel_store_rate_store()
            .call_load_rates(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(LoadRatesResponse {
            rates: wit_rates.into_iter().map(|r| ProtoRatePlan {
                hotel_id: r.hotel_id,
                code: r.code,
                in_date: r.in_date,
                out_date: r.out_date,
                room_type: Some(ProtoRoomType {
                    bookable_rate: r.room_type.bookable_rate,
                    code: r.room_type.code,
                    room_description: r.room_type.room_description,
                    total_rate: r.room_type.total_rate,
                    total_rate_inclusive: r.room_type.total_rate_inclusive,
                }),
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    RateStoreHostWorld, RateStoreHostWorldPre<host_lib::StoreData>, RateStoreServer,
    "rate-db",
    "0.0.0.0:8094", "rate-store.wasm", "rate-host"
);
