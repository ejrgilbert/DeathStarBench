use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

mod proto {
    tonic::include_proto!("profile");
}
use proto::profile_server::{Profile, ProfileServer};

pub use proto::{Hotel, Address, Image};

#[async_trait::async_trait]
pub trait ProfileComponent: Send + Sync + 'static {
    async fn get_profiles(&self, hotel_ids: Vec<String>) -> Result<Vec<Hotel>>;
}

pub struct ProfileService<C: ProfileComponent> {
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: ProfileComponent> Profile for ProfileService<C> {
    async fn get_profiles(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let hotels = self.component
            .get_profiles(r.hotel_ids)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { hotels }))
    }
}

pub async fn serve<C: ProfileComponent>(component: Arc<C>, addr: SocketAddr) -> Result<()> {
    Server::builder()
        .add_service(ProfileServer::new(ProfileService { component }))
        .serve(addr)
        .await?;
    Ok(())
}
