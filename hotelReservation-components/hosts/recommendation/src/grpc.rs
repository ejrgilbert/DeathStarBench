use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("recommendation");
}
use proto::recommendation_server::{Recommendation, RecommendationServer};

pub enum Requirement {
    Distance,
    Rate,
    Price,
}

/// Implemented by each mode's bindgen!-generated world type to call into the wasm component.
#[async_trait::async_trait]
pub trait RecommendComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn recommend(
        &self,
        store: &mut Store<Self::Data>,
        requirement: Requirement,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>>;
}

pub struct RecommendService<C: RecommendComponent> {
    pub store: Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: RecommendComponent> Recommendation for RecommendService<C> {
    async fn get_recommendations(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let requirement = match r.require.as_str() {
            "dis" | "distance" => Requirement::Distance,
            "rate"             => Requirement::Rate,
            "price"            => Requirement::Price,
            other => return Err(Status::invalid_argument(format!("unknown require: {other}"))),
        };
        let mut store = self.store.lock().await;
        let hotel_ids = self
            .component
            .recommend(&mut *store, requirement, r.lat, r.lon)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { hotel_ids }))
    }
}

pub async fn serve<C: RecommendComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(RecommendationServer::new(RecommendService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
