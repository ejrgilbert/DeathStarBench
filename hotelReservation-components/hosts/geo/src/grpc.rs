use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("geo");
}
use proto::geo_server::{Geo, GeoServer};

#[async_trait::async_trait]
pub trait GeoComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn nearby(
        &self,
        store: &mut Store<Self::Data>,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>>;
}

pub struct GeoService<C: GeoComponent> {
    pub store:     Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: GeoComponent> Geo for GeoService<C> {
    async fn nearby(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let mut store = self.store.lock().await;
        let ids = self.component
            .nearby(&mut *store, r.lat, r.lon)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { ids }))
    }
}

pub async fn serve<C: GeoComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(GeoServer::new(GeoService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
