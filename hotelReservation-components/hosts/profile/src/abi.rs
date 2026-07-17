use anyhow::Result;
use wasmtime::Store;
use crate::grpc::{ProfileComponent, Hotel, Address, Image};

wasmtime::component::bindgen!({
    path: "../../components/profile/wit",
    world: "profile-composed-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

use host_lib::StoreData;

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl ProfileComponent for ProfileComposedHostWorld {
    type Data = StoreData;

    async fn get_profiles(
        &self,
        store: &mut Store<StoreData>,
        hotel_ids: Vec<String>,
    ) -> Result<Vec<Hotel>> {
        let wit_hotels = self.hotel_api_profile()
            .call_get_profiles(store, &hotel_ids).await?;
        Ok(wit_hotels.into_iter().map(|p| Hotel {
            id:           p.id,
            name:         p.name,
            phone_number: p.phone_number,
            description:  p.description,
            address: Some(Address {
                street_number: p.addr.street_number,
                street_name:   p.addr.street_name,
                city:          p.addr.city,
                state:         p.addr.state,
                country:       p.addr.country,
                postal_code:   p.addr.postal_code,
                lat:           p.addr.lat as f32,
                lon:           p.addr.lon as f32,
            }),
            images: p.images.into_iter().map(|img| Image {
                url:     img.url,
                default: img.default,
            }).collect(),
        }).collect())
    }
}

host_lib::run_abi!(
    ProfileComposedHostWorld, hotel_api_profile,
    "profile-db",
    "0.0.0.0:8095", "profile-composed.wasm", "profile-host"
);
