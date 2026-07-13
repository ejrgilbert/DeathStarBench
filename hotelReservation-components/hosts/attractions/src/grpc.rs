use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("attractions");
}
use proto::attractions_server::{Attractions, AttractionsServer};

/// Implemented by each mode's bindgen!-generated world type to call into the wasm component.
#[async_trait::async_trait]
pub trait AttractionsComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn nearby_rest(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>>;

    async fn nearby_mus(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>>;

    async fn nearby_cinema(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>>;
}

pub struct AttractionsService<C: AttractionsComponent> {
    pub store:     Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: AttractionsComponent> Attractions for AttractionsService<C> {
    async fn nearby_rest(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let hotel_id = req.into_inner().hotel_id;
        let mut store = self.store.lock().await;
        let ids = self.component
            .nearby_rest(&mut *store, hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { attraction_ids: ids }))
    }

    async fn nearby_mus(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let hotel_id = req.into_inner().hotel_id;
        let mut store = self.store.lock().await;
        let ids = self.component
            .nearby_mus(&mut *store, hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { attraction_ids: ids }))
    }

    async fn nearby_cinema(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let hotel_id = req.into_inner().hotel_id;
        let mut store = self.store.lock().await;
        let ids = self.component
            .nearby_cinema(&mut *store, hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { attraction_ids: ids }))
    }
}

pub async fn serve<C: AttractionsComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(AttractionsServer::new(AttractionsService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
