use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("review");
}
use proto::review_server::{Review, ReviewServer};

pub use proto::{ReviewComm, Image};

#[async_trait::async_trait]
pub trait ReviewComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn get_reviews(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<ReviewComm>>;
}

pub struct ReviewService<C: ReviewComponent> {
    pub store:     Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: ReviewComponent> Review for ReviewService<C> {
    async fn get_reviews(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let mut store = self.store.lock().await;
        let reviews = self.component
            .get_reviews(&mut *store, r.hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { reviews }))
    }
}

pub async fn serve<C: ReviewComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(ReviewServer::new(ReviewService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
