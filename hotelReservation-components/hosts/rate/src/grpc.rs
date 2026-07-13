use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("rate");
}
use proto::rate_server::{Rate, RateServer};

pub use proto::{RatePlan, RoomType};

#[async_trait::async_trait]
pub trait RateComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn get_rates(
        &self,
        store: &mut Store<Self::Data>,
        hotel_ids: Vec<String>,
        in_date: String,
        out_date: String,
    ) -> Result<Vec<RatePlan>>;
}

pub struct RateService<C: RateComponent> {
    pub store:     Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: RateComponent> Rate for RateService<C> {
    async fn get_rates(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let mut store = self.store.lock().await;
        let plans = self.component
            .get_rates(&mut *store, r.hotel_ids, r.in_date, r.out_date)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { rate_plans: plans }))
    }
}

pub async fn serve<C: RateComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(RateServer::new(RateService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
