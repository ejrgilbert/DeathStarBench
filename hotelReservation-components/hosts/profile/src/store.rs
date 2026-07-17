use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("profile_store");
}
use proto::{
    profile_store_server::{ProfileStore, ProfileStoreServer},
    Hotel as ProtoHotel, Address as ProtoAddress, Image as ProtoImage,
    InitRequest, InitResponse, LoadProfilesRequest, LoadProfilesResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/profile_store/wit",
    world: "profile-store-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(ProfileStoreHostWorld);

#[tonic::async_trait]
impl ProfileStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_store_profile_store()
            .call_init(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_profiles(
        &self,
        _req: Request<LoadProfilesRequest>,
    ) -> Result<Response<LoadProfilesResponse>, Status> {
        let mut store = self.store.lock().await;
        let wit_hotels = self.instance
            .hotel_store_profile_store()
            .call_load_profiles(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadProfilesResponse {
            profs: wit_hotels.into_iter().map(|h| ProtoHotel {
                id:           h.id,
                name:         h.name,
                phone_number: h.phone_number,
                description:  h.description,
                address: Some(ProtoAddress {
                    street_number: h.addr.street_number,
                    street_name:   h.addr.street_name,
                    city:          h.addr.city,
                    state:         h.addr.state,
                    country:       h.addr.country,
                    postal_code:   h.addr.postal_code,
                    lat:           h.addr.lat as f32,
                    lon:           h.addr.lon as f32,
                }),
                images: h.images.into_iter().map(|img| ProtoImage {
                    url:     img.url,
                    default: img.default,
                }).collect(),
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    ProfileStoreHostWorld, ProfileStoreServer, hotel_store_profile_store,
    "profile-db",
    "0.0.0.0:8096", "profile-store.wasm", "profile-host"
);
