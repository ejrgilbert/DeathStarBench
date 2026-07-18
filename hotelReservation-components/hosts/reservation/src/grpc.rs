use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

mod proto {
    tonic::include_proto!("reservation");
}
use proto::reservation_server::{Reservation, ReservationServer};

#[async_trait::async_trait]
pub trait ReservationComponent: Send + Sync + 'static {
    async fn check_availability(
        &self,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>>;

    async fn make_reservation(
        &self,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>>;
}

pub struct ReservationService<C: ReservationComponent> {
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: ReservationComponent> Reservation for ReservationService<C> {
    async fn check_availability(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let hotel_id = self.component
            .check_availability(r.hotel_id, r.in_date, r.out_date, r.room_number)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { hotel_id }))
    }

    async fn make_reservation(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let hotel_id_single = r.hotel_id.into_iter().next().unwrap_or_default();
        let hotel_id = self.component
            .make_reservation(
                hotel_id_single, r.customer_name, r.in_date, r.out_date, r.room_number,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { hotel_id }))
    }
}

pub async fn serve<C: ReservationComponent>(component: Arc<C>, addr: SocketAddr) -> Result<()> {
    Server::builder()
        .add_service(ReservationServer::new(ReservationService { component }))
        .serve(addr)
        .await?;
    Ok(())
}
