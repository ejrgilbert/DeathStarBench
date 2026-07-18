use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

mod proto {
    tonic::include_proto!("attractions");
}
use proto::attractions_server::{Attractions, AttractionsServer};

#[async_trait::async_trait]
pub trait AttractionsComponent: Send + Sync + 'static {
    async fn nearby_rest(&self, hotel_id: String) -> Result<Vec<String>>;
    async fn nearby_mus(&self, hotel_id: String) -> Result<Vec<String>>;
    async fn nearby_cinema(&self, hotel_id: String) -> Result<Vec<String>>;
}

pub struct AttractionsService<C: AttractionsComponent> {
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: AttractionsComponent> Attractions for AttractionsService<C> {
    async fn nearby_rest(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let hotel_id = req.into_inner().hotel_id;
        let ids = self.component
            .nearby_rest(hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { attraction_ids: ids }))
    }

    async fn nearby_mus(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let hotel_id = req.into_inner().hotel_id;
        let ids = self.component
            .nearby_mus(hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { attraction_ids: ids }))
    }

    async fn nearby_cinema(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let hotel_id = req.into_inner().hotel_id;
        let ids = self.component
            .nearby_cinema(hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { attraction_ids: ids }))
    }
}

pub async fn serve<C: AttractionsComponent>(component: Arc<C>, addr: SocketAddr) -> Result<()> {
    Server::builder()
        .add_service(AttractionsServer::new(AttractionsService { component }))
        .serve(addr)
        .await?;
    Ok(())
}
