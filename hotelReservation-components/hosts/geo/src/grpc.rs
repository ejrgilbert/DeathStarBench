use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

mod proto {
    tonic::include_proto!("geo");
}
use proto::geo_server::{Geo, GeoServer};

#[async_trait::async_trait]
pub trait GeoComponent: Send + Sync + 'static {
    async fn nearby(&self, lat: f64, lon: f64) -> Result<Vec<String>>;
}

pub struct GeoService<C: GeoComponent> {
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: GeoComponent> Geo for GeoService<C> {
    async fn nearby(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let ids = self.component
            .nearby(r.lat, r.lon)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { ids }))
    }
}

pub async fn serve<C: GeoComponent>(component: Arc<C>, addr: SocketAddr) -> Result<()> {
    Server::builder()
        .add_service(GeoServer::new(GeoService { component }))
        .serve(addr)
        .await?;
    Ok(())
}
