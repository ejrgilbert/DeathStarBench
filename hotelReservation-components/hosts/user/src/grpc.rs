use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

mod proto {
    tonic::include_proto!("user");
}
use proto::user_server::{User, UserServer};

#[async_trait::async_trait]
pub trait UserComponent: Send + Sync + 'static {
    async fn check_user(&self, username: String, password: String) -> Result<bool>;
}

pub struct UserService<C: UserComponent> {
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: UserComponent> User for UserService<C> {
    async fn check_user(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let correct = self.component
            .check_user(r.username, r.password)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { correct }))
    }
}

pub async fn serve<C: UserComponent>(component: Arc<C>, addr: SocketAddr) -> Result<()> {
    Server::builder()
        .add_service(UserServer::new(UserService { component }))
        .serve(addr)
        .await?;
    Ok(())
}
