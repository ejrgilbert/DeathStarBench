use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

mod proto {
    tonic::include_proto!("review");
}
use proto::review_server::{Review, ReviewServer};

pub use proto::{ReviewComm, Image};

#[async_trait::async_trait]
pub trait ReviewComponent: Send + Sync + 'static {
    async fn get_reviews(&self, hotel_id: String) -> Result<Vec<ReviewComm>>;
}

pub struct ReviewService<C: ReviewComponent> {
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: ReviewComponent> Review for ReviewService<C> {
    async fn get_reviews(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let reviews = self.component
            .get_reviews(r.hotel_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { reviews }))
    }
}

pub async fn serve<C: ReviewComponent>(component: Arc<C>, addr: SocketAddr) -> Result<()> {
    Server::builder()
        .add_service(ReviewServer::new(ReviewService { component }))
        .serve(addr)
        .await?;
    Ok(())
}
