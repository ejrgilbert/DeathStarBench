use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

mod proto {
    tonic::include_proto!("search");
}
use proto::search_server::{Search, SearchServer};

#[async_trait::async_trait]
pub trait SearchComponent: Send + Sync + 'static {
    async fn nearby(
        &self,
        lat:      f64,
        lon:      f64,
        in_date:  String,
        out_date: String,
    ) -> Result<Vec<String>>;
}

pub struct SearchService<C: SearchComponent> {
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: SearchComponent> Search for SearchService<C> {
    async fn nearby(
        &self,
        req: Request<proto::NearbyRequest>,
    ) -> Result<Response<proto::SearchResult>, Status> {
        let r = req.into_inner();
        let hotel_ids = self.component
            .nearby(r.lat as f64, r.lon as f64, r.in_date, r.out_date)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::SearchResult { hotel_ids }))
    }
}

pub async fn serve<C: SearchComponent>(component: Arc<C>, addr: SocketAddr) -> Result<()> {
    Server::builder()
        .add_service(SearchServer::new(SearchService { component }))
        .serve(addr)
        .await?;
    Ok(())
}
