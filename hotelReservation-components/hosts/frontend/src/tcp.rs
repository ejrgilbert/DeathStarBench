use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tonic::transport::Channel;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView};
use wasmtime_wasi_http::WasiHttpCtx;
use wasmtime_wasi_http::p2::{WasiHttpView, WasiHttpCtxView};
use wasmtime_wasi_http::p2::bindings::{Proxy, ProxyPre};

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

// Host-side world: only the custom (gRPC-backed) `hotel:api` imports. The
// wasi:http proxy export is driven through the crate's `ProxyPre` (see `run` /
// `handle_request`), and the wasi + wasi:http host functions are provided by the
// crate's linker helpers. Generating bindings only for our own imports avoids
// the `@unstable`-gated wasi:http/types linker glue that the full proxy world
// would emit.
wasmtime::component::bindgen!({
    inline: "
        package hotel:frontend-host;
        world frontend-imports {
            import hotel:api/search;
            import hotel:api/profile;
            import hotel:api/recommendation;
            import hotel:api/user;
            import hotel:api/review;
            import hotel:api/attractions;
            import hotel:api/reservation;
        }
    ",
    path: "../../components/frontend/wit",
    imports: { default: async },
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
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl WasiHttpView for HostData {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView { ctx: &mut self.http, table: &mut self.table, hooks: Default::default() }
    }
}

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
    pool: host_lib::InstancePool<HostData, Proxy>,
}

async fn handle_request(
    state: Arc<ServerState>,
    req:   hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<wasmtime_wasi_http::p2::body::HyperOutgoingBody>> {
    use http_body_util::BodyExt;

    // Reuse a warm instance from the pool instead of instantiating per request.
    let mut checked = state.pool.checkout().await;
    let (store, proxy) = checked.parts();

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let scheme   = wasmtime_wasi_http::p2::bindings::http::types::Scheme::Http;
    let incoming = store.data_mut().http().new_incoming_request(scheme, req)?;
    let outparam = store.data_mut().http().new_response_outparam(sender)?;

    if let Err(e) = proxy
        .wasi_http_incoming_handler()
        .call_handle(&mut *store, incoming, outparam)
        .await
    {
        eprintln!("[reuse-debug] call_handle trapped: {e:?}");
        return Err(e.into());
    }

    let resp = match receiver.await? {
        Ok(resp) => resp,
        Err(e)   => return Err(anyhow::anyhow!("wasi:http error code: {e:?}")),
    };

    // Fully drain the response body while the instance is still checked out, so
    // the returned response no longer references the store — otherwise the
    // instance would go back to the pool mid-stream and a concurrent request
    // re-entering it would trap ("cannot enter component instance").
    let (parts, body) = resp.into_parts();
    let bytes = match body.collect().await {
        Ok(c) => c.to_bytes(),
        Err(e) => {
            eprintln!("[reuse-debug] body drain failed: {e:?}");
            return Err(anyhow::anyhow!("draining response body: {e:?}"));
        }
    };
    // The call returned cleanly and no borrow of the store remains: commit so the
    // warm instance is returned to the pool. On any earlier error path (or if the
    // request future is cancelled), `checked` drops uncommitted and the pool
    // rebuilds a fresh instance instead of recycling a poisoned one.
    checked.commit();

    let body = http_body_util::Full::new(bytes)
        .map_err(|never| match never {})
        .boxed_unsync();
    Ok(hyper::Response::from_parts(parts, body))
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
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)?;
    hotel::api::search::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
    hotel::api::profile::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
    hotel::api::recommendation::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
    hotel::api::user::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
    hotel::api::review::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
    hotel::api::attractions::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
    hotel::api::reservation::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(ProxyPre::new(linker.instantiate_pre(&component)?)?);
    let engine = Arc::new(engine);

    // Pre-instantiate a pool of warm instances reused across requests (bounds
    // in-flight concurrency); avoids the per-request `instantiate_async` cost.
    let pool_size: usize = env_or("POOL_SIZE", "64").parse().unwrap_or(64);
    let pool = host_lib::InstancePool::build(pool_size, move || {
        let engine = engine.clone();
        let pre = pre.clone();
        let clients = clients.clone();
        async move {
            let data = HostData {
                wasi:    WasiCtxBuilder::new().inherit_env().inherit_stderr().build(),
                table:   ResourceTable::new(),
                http:    WasiHttpCtx::new(),
                clients,
            };
            let mut store = Store::new(&engine, data);
            let proxy = pre.instantiate_async(&mut store).await?;
            Ok((store, proxy))
        }
    })
    .await?;

    let state = Arc::new(ServerState { pool });

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    println!("frontend-host [tcp] listening on {listen_addr} (instance pool size {pool_size})");

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
