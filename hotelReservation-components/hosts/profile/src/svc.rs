use anyhow::Result;
use crate::grpc::{ProfileComponent, Hotel, Address, Image};

mod store_proto {
    tonic::include_proto!("profile_store");
}
use store_proto::{profile_store_client::ProfileStoreClient, LoadProfilesRequest};

wasmtime::component::bindgen!({
    path: "../../components/profile/wit",
    world: "profile-host-world",
    imports: { default: async },
    exports: { default: async },
});

host_lib::svc_host_data!(ProfileStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_svc_grpc!(HostData);

impl hotel::store::profile_store::Host for HostData {
    async fn load_profiles(&mut self) -> Vec<hotel::store::profile_store::Hotel> {
        let resp = self.store_client.clone()
            .load_profiles(tonic::Request::new(LoadProfilesRequest {})).await
            .expect("gRPC profile-store LoadProfiles failed").into_inner();
        resp.profs.into_iter().map(|h| {
            let a = h.address.unwrap_or_default();
            hotel::store::profile_store::Hotel {
                id:           h.id,
                name:         h.name,
                phone_number: h.phone_number,
                description:  h.description,
                addr: hotel::store::profile_store::Address {
                    street_number: a.street_number,
                    street_name:   a.street_name,
                    city:          a.city,
                    state:         a.state,
                    country:       a.country,
                    postal_code:   a.postal_code,
                    lat:           a.lat as f64,
                    lon:           a.lon as f64,
                },
                images: h.images.into_iter().map(|img| hotel::store::profile_store::Image {
                    url:     img.url,
                    default: img.default,
                }).collect(),
            }
        }).collect()
    }

    // Targeted single-hotel lookup via the store's GetProfile RPC (Mongo FindOne)
    // — a real per-request query on a cache miss, matching the native profile
    // service. (The ABI path uses the composed store's `get-profile` directly.)
    async fn get_profile(&mut self, id: String) -> Option<hotel::store::profile_store::Hotel> {
        let resp = self.store_client.clone()
            .get_profile(tonic::Request::new(store_proto::GetProfileRequest { id })).await
            .ok()?
            .into_inner();
        if !resp.found {
            return None;
        }
        let h = resp.prof?;
        let a = h.address.unwrap_or_default();
        Some(hotel::store::profile_store::Hotel {
            id:           h.id,
            name:         h.name,
            phone_number: h.phone_number,
            description:  h.description,
            addr: hotel::store::profile_store::Address {
                street_number: a.street_number,
                street_name:   a.street_name,
                city:          a.city,
                state:         a.state,
                country:       a.country,
                postal_code:   a.postal_code,
                lat:           a.lat as f64,
                lon:           a.lon as f64,
            },
            images: h.images.into_iter().map(|img| hotel::store::profile_store::Image {
                url:     img.url,
                default: img.default,
            }).collect(),
        })
    }
}

type ProfileSvcHost = host_lib::PooledSvcHost<ProfileStoreClient<tonic::transport::Channel>, ProfileHostWorld>;

#[async_trait::async_trait]
impl ProfileComponent for ProfileSvcHost {
    async fn get_profiles(&self, hotel_ids: Vec<String>) -> Result<Vec<Hotel>> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let wit_hotels = instance.hotel_api_profile()
            .call_get_profiles(&mut *store, &hotel_ids).await?;
        checked.commit();
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

host_lib::run_svc_pre!(
    ProfileHostWorld, ProfileHostWorldPre<HostData>, ProfileStoreClient<tonic::transport::Channel>,
    "http://localhost:8096", "0.0.0.0:8095", "profile.wasm", "profile-host [svc]"
);
