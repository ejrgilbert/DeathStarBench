use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("profile_store");
}
use proto::{
    profile_store_server::{ProfileStore, ProfileStoreServer},
    Hotel as ProtoHotel, Address as ProtoAddress, Image as ProtoImage,
    LoadProfilesRequest, LoadProfilesResponse,
    GetProfileRequest, GetProfileResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/profile_store/wit",
    world: "profile-store-host-world",
    imports: { default: async },
    exports: { default: async },
    with: {
        "host:storage/collection.connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(ProfileStoreHostWorld, ProfileStoreHostWorldPre<host_lib::StoreData>);

#[tonic::async_trait]
impl ProfileStore for StoreGrpcService {
    async fn load_profiles(
        &self,
        _req: Request<LoadProfilesRequest>,
    ) -> Result<Response<LoadProfilesResponse>, Status> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let wit_hotels = instance
            .hotel_store_profile_store()
            .call_load_profiles(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
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

    // Targeted single-hotel lookup: a real per-request Mongo FindOne in the store
    // guest, matching native. No full-collection load / preload.
    async fn get_profile(
        &self,
        req: Request<GetProfileRequest>,
    ) -> Result<Response<GetProfileResponse>, Status> {
        let id = req.into_inner().id;
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let opt = instance
            .hotel_store_profile_store()
            .call_get_profile(&mut *store, &id).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(match opt {
            Some(h) => GetProfileResponse {
                found: true,
                prof: Some(ProtoHotel {
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
                }),
            },
            None => GetProfileResponse { found: false, prof: None },
        }))
    }
}

host_lib::run_store!(
    ProfileStoreHostWorld, ProfileStoreHostWorldPre<host_lib::StoreData>, ProfileStoreServer,
    "profile-db",
    "0.0.0.0:8096", "profile-store.wasm", "profile-host"
);
