use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tonic::transport::Channel;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

mod search_proto {
    tonic::include_proto!("search");
}
mod profile_proto {
    tonic::include_proto!("profile");
}
mod recommendation_proto {
    tonic::include_proto!("recommendation");
}
mod user_proto {
    tonic::include_proto!("user");
}
mod review_proto {
    tonic::include_proto!("review");
}
mod attractions_proto {
    tonic::include_proto!("attractions");
}
mod reservation_proto {
    tonic::include_proto!("reservation");
}

use attractions_proto::attractions_client::AttractionsClient;
use profile_proto::profile_client::ProfileClient;
use recommendation_proto::recommendation_client::RecommendationClient;
use reservation_proto::reservation_client::ReservationClient;
use review_proto::review_client::ReviewClient;
use search_proto::search_client::SearchClient;
use user_proto::user_client::UserClient;

wasmtime::component::bindgen!({
    world: "frontend-host-world",
    path: "../../components/frontend/wit",
    with: {
        "wasi:http/types@0.2.0":               wasmtime_wasi_http::bindings::http::types,
        "wasi:io/poll@0.2.0":                  wasmtime_wasi::bindings::io::poll,
        "wasi:io/error@0.2.0":                 wasmtime_wasi::bindings::io::error,
        "wasi:io/streams@0.2.0":               wasmtime_wasi::bindings::io::streams,
        "wasi:clocks/monotonic-clock@0.2.0":   wasmtime_wasi::bindings::clocks::monotonic_clock,
    },
    async: true,
});

struct Clients {
    search:         SearchClient<Channel>,
    profile:        ProfileClient<Channel>,
    recommendation: RecommendationClient<Channel>,
    user:           UserClient<Channel>,
    review:         ReviewClient<Channel>,
    attractions:    AttractionsClient<Channel>,
    reservation:    ReservationClient<Channel>,
}

struct HostData {
    wasi:    WasiCtx,
    table:   ResourceTable,
    http:    WasiHttpCtx,
    clients: Arc<Clients>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

impl WasiHttpView for HostData {
    fn ctx(&mut self)   -> &mut WasiHttpCtx  { &mut self.http  }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::api::search::Host for HostData {
    async fn nearby(
        &mut self,
        lat:      f64,
        lon:      f64,
        in_date:  String,
        out_date: String,
    ) -> Vec<String> {
        self.clients
            .search
            .clone()
            .nearby(search_proto::NearbyRequest {
                lat: lat as f32,
                lon: lon as f32,
                in_date,
                out_date,
            })
            .await
            .map(|r| r.into_inner().hotel_ids)
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl hotel::api::profile::Host for HostData {
    async fn get_profiles(
        &mut self,
        hotel_ids: Vec<String>,
    ) -> Vec<hotel::api::profile::Hotel> {
        self.clients
            .profile
            .clone()
            .get_profiles(profile_proto::Request {
                hotel_ids,
                locale: String::new(),
            })
            .await
            .map(|r| r.into_inner().hotels.into_iter().map(proto_hotel_to_wit).collect())
            .unwrap_or_default()
    }
}

fn proto_hotel_to_wit(h: profile_proto::Hotel) -> hotel::api::profile::Hotel {
    let a = h.address.as_ref();
    hotel::api::profile::Hotel {
        id:           h.id,
        name:         h.name,
        phone_number: h.phone_number,
        description:  h.description,
        addr: hotel::api::profile::Address {
            street_number: a.map(|x| x.street_number.clone()).unwrap_or_default(),
            street_name:   a.map(|x| x.street_name.clone()).unwrap_or_default(),
            city:          a.map(|x| x.city.clone()).unwrap_or_default(),
            state:         a.map(|x| x.state.clone()).unwrap_or_default(),
            country:       a.map(|x| x.country.clone()).unwrap_or_default(),
            postal_code:   a.map(|x| x.postal_code.clone()).unwrap_or_default(),
            lat:           a.map(|x| x.lat as f64).unwrap_or_default(),
            lon:           a.map(|x| x.lon as f64).unwrap_or_default(),
        },
        images: h
            .images
            .into_iter()
            .map(|i| hotel::api::profile::Image { url: i.url, default: i.default })
            .collect(),
    }
}

#[async_trait::async_trait]
impl hotel::api::recommendation::Host for HostData {
    async fn recommend(
        &mut self,
        requirement: hotel::api::recommendation::Requirement,
        lat:         f64,
        lon:         f64,
    ) -> Vec<String> {
        use hotel::api::recommendation::Requirement::*;
        let require = match requirement {
            Distance => "dis",
            Rate     => "rate",
            Price    => "price",
        };
        self.clients
            .recommendation
            .clone()
            .get_recommendations(recommendation_proto::Request {
                require: require.to_string(),
                lat,
                lon,
            })
            .await
            .map(|r| r.into_inner().hotel_ids)
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl hotel::api::user::Host for HostData {
    async fn check_user(&mut self, username: String, password: String) -> bool {
        self.clients
            .user
            .clone()
            .check_user(user_proto::Request { username, password })
            .await
            .map(|r| r.into_inner().correct)
            .unwrap_or(false)
    }
}

#[async_trait::async_trait]
impl hotel::api::review::Host for HostData {
    async fn get_reviews(
        &mut self,
        hotel_id: String,
    ) -> Vec<hotel::api::review::ReviewComm> {
        self.clients
            .review
            .clone()
            .get_reviews(review_proto::Request { hotel_id })
            .await
            .map(|r| {
                r.into_inner()
                    .reviews
                    .into_iter()
                    .map(|rv| hotel::api::review::ReviewComm {
                        review_id:   rv.review_id,
                        hotel_id:    rv.hotel_id,
                        name:        rv.name,
                        rating:      rv.rating,
                        description: rv.description,
                        image: hotel::api::review::Image {
                            url:     rv.images.as_ref().map(|i| i.url.clone()).unwrap_or_default(),
                            default: rv.images.as_ref().map(|i| i.default).unwrap_or_default(),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl hotel::api::attractions::Host for HostData {
    async fn nearby_rest(&mut self, hotel_id: String) -> Vec<String> {
        self.clients
            .attractions
            .clone()
            .nearby_rest(attractions_proto::Request { hotel_id })
            .await
            .map(|r| r.into_inner().attraction_ids)
            .unwrap_or_default()
    }

    async fn nearby_mus(&mut self, hotel_id: String) -> Vec<String> {
        self.clients
            .attractions
            .clone()
            .nearby_mus(attractions_proto::Request { hotel_id })
            .await
            .map(|r| r.into_inner().attraction_ids)
            .unwrap_or_default()
    }

    async fn nearby_cinema(&mut self, hotel_id: String) -> Vec<String> {
        self.clients
            .attractions
            .clone()
            .nearby_cinema(attractions_proto::Request { hotel_id })
            .await
            .map(|r| r.into_inner().attraction_ids)
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl hotel::api::reservation::Host for HostData {
    async fn check_availability(
        &mut self,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Vec<String> {
        self.clients
            .reservation
            .clone()
            .check_availability(reservation_proto::Request {
                customer_name: String::new(),
                hotel_id:      hotel_ids,
                in_date,
                out_date,
                room_number,
            })
            .await
            .map(|r| r.into_inner().hotel_id)
            .unwrap_or_default()
    }

    async fn make_reservation(
        &mut self,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Vec<String> {
        self.clients
            .reservation
            .clone()
            .make_reservation(reservation_proto::Request {
                customer_name,
                hotel_id: vec![hotel_id],
                in_date,
                out_date,
                room_number,
            })
            .await
            .map(|r| r.into_inner().hotel_id)
            .unwrap_or_default()
    }
}

struct ServerState {
    engine:  wasmtime::Engine,
    pre:     FrontendHostWorldPre<HostData>,
    clients: Arc<Clients>,
}

async fn handle_request(
    state: Arc<ServerState>,
    req:   hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>> {
    let data = HostData {
        wasi:    WasiCtxBuilder::new().inherit_stderr().build(),
        table:   ResourceTable::new(),
        http:    WasiHttpCtx::new(),
        clients: state.clients.clone(),
    };
    let mut store = Store::new(&state.engine, data);

    let instance = state.pre.instantiate_async(&mut store).await?;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let scheme   = wasmtime_wasi_http::bindings::http::types::Scheme::Http;
    let incoming = store.data_mut().new_incoming_request(scheme, req)?;
    let outparam = store.data_mut().new_response_outparam(sender)?;

    instance
        .wasi_http_incoming_handler()
        .call_handle(&mut store, incoming, outparam)
        .await?;

    match receiver.await? {
        Ok(resp) => Ok(resp),
        Err(e)   => Err(anyhow::anyhow!("wasi:http error code: {e:?}")),
    }
}

pub async fn run() -> Result<()> {
    fn env_or(k: &str, d: &str) -> String {
        std::env::var(k).unwrap_or_else(|_| d.to_string())
    }

    let wasm_file        = env_or("WASM_FILE",         "frontend.wasm");
    let listen_addr: SocketAddr = env_or("LISTEN_ADDR", "0.0.0.0:8080").parse()?;
    let search_addr      = env_or("SEARCH_ADDR",         "http://localhost:8097");
    let profile_addr     = env_or("PROFILE_ADDR",        "http://localhost:8095");
    let rec_addr         = env_or("RECOMMENDATION_ADDR", "http://localhost:8085");
    let user_addr        = env_or("USER_ADDR",           "http://localhost:8091");
    let review_addr      = env_or("REVIEW_ADDR",         "http://localhost:8098");
    let attr_addr        = env_or("ATTRACTIONS_ADDR",    "http://localhost:8087");
    let reservation_addr = env_or("RESERVATION_ADDR",   "http://localhost:8100");

    let clients = Arc::new(Clients {
        search:         SearchClient::connect(search_addr).await?,
        profile:        ProfileClient::connect(profile_addr).await?,
        recommendation: RecommendationClient::connect(rec_addr).await?,
        user:           UserClient::connect(user_addr).await?,
        review:         ReviewClient::connect(review_addr).await?,
        attractions:    AttractionsClient::connect(attr_addr).await?,
        reservation:    ReservationClient::connect(reservation_addr).await?,
    });

    let engine = host_lib::make_engine()?;

    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;
    hotel::api::search::add_to_linker(&mut linker, |d| d)?;
    hotel::api::profile::add_to_linker(&mut linker, |d| d)?;
    hotel::api::recommendation::add_to_linker(&mut linker, |d| d)?;
    hotel::api::user::add_to_linker(&mut linker, |d| d)?;
    hotel::api::review::add_to_linker(&mut linker, |d| d)?;
    hotel::api::attractions::add_to_linker(&mut linker, |d| d)?;
    hotel::api::reservation::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = FrontendHostWorldPre::new(linker.instantiate_pre(&component)?)?;

    let state = Arc::new(ServerState { engine, pre, clients });

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    println!("frontend-host [tcp] listening on {listen_addr}");

    loop {
        let (tcp, _) = listener.accept().await?;
        let io = TokioIo::new(tcp);
        let state = state.clone();

        tokio::task::spawn(async move {
            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| {
                        let state = state.clone();
                        async move { handle_request(state, req).await }
                    }),
                )
                .await
            {
                eprintln!("connection error: {e}");
            }
        });
    }
}
