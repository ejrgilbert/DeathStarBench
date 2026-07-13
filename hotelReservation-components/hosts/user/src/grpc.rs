use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("user");
}
use proto::user_server::{User, UserServer};

#[async_trait::async_trait]
pub trait UserComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn check_user(
        &self,
        store: &mut Store<Self::Data>,
        username: String,
        password: String,
    ) -> Result<bool>;
}

pub struct UserService<C: UserComponent> {
    pub store:     Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: UserComponent> User for UserService<C> {
    async fn check_user(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let mut store = self.store.lock().await;
        let correct = self.component
            .check_user(&mut *store, r.username, r.password)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { correct }))
    }
}

pub async fn serve<C: UserComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(UserServer::new(UserService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
