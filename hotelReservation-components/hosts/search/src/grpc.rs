use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("search");
}
use proto::search_server::{Search, SearchServer};

#[async_trait::async_trait]
pub trait SearchComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn nearby(
        &self,
        store: &mut Store<Self::Data>,
        lat: f64,
        lon: f64,
        in_date: String,
        out_date: String,
    ) -> Result<Vec<String>>;
}

pub struct SearchService<C: SearchComponent> {
    pub store:     Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: SearchComponent> Search for SearchService<C> {
    async fn nearby(
        &self,
        req: Request<proto::NearbyRequest>,
    ) -> Result<Response<proto::SearchResult>, Status> {
        let r = req.into_inner();
        let mut store = self.store.lock().await;
        let hotel_ids = self.component
            .nearby(&mut *store, r.lat as f64, r.lon as f64, r.in_date, r.out_date)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::SearchResult { hotel_ids }))
    }
}

pub async fn serve<C: SearchComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(SearchServer::new(SearchService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
