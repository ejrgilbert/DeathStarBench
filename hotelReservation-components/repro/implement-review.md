# Task: Implement the `review` service

Working directory: `eval/benchmarks/msa/DeathStarBench/hotelReservation-components/`

## Context

This repo is a port of the DeathStarBench hotelReservation microservices to WebAssembly components
(TinyGo + Wasmtime). Each service is split into:
- A TinyGo **component** (`components/<svc>/`) that holds business logic
- A Rust **host** (`hosts/<svc>/`) that embeds the component in Wasmtime and serves gRPC
- Most services also have a **store** component (`components/<svc>_store/`) that owns MongoDB access

The `review` service follows the standard store/svc/abi triple-mode pattern, identical in structure
to `profile` and `rate`.

The original Go implementation is at `../hotelReservation/services/review/server.go`.
Its `GetReviews` handler:
1. Checks memcache for `hotelId` → if hit, deserialize and return
2. If miss, queries MongoDB `review-db.reviews` for docs matching `{hotelId: hotelId}`
3. Caches the result in memcache

In our port, memcache is replaced by `host:cache/keyvalue` (in-memory HashMap in the Rust host),
and MongoDB access is done via the load-all + filter-in-Go pattern used by `profile` and `rate`.

## Important TinyGo WIT string rule

Every string coming back from a WIT import call **must** be copied with `string([]byte(s))` before
storing or passing onward. The GC can collect the `cabi_realloc` buffer the string header points
into if you don't copy. See `components/rate/main.go:loadAll()` for examples throughout.

## Reference implementations

The `profile` service is the closest reference; study these files before implementing:
- `components/profile_store/wit/profile-store.wit` — store WIT structure
- `components/profile_store/main.go` — seed-load + MongoDB pattern
- `components/profile/wit/profile.wit` — svc WIT with cache import
- `components/profile/service.go` — pure-Go business logic with cache
- `components/profile/main.go` — WIT glue + string copy pattern
- `hosts/profile/src/grpc.rs` — gRPC service trait
- `hosts/profile/src/store.rs` — store host using `run_store!` macro
- `hosts/profile/src/svc.rs` — svc host with store + cache client
- `hosts/profile/src/abi.rs` — abi host with direct MongoDB + cache

## Port assignments

| Service      | svc port | store port |
|--------------|----------|------------|
| recommendation | 8085   | 8086       |
| attractions  | 8087     | 8088       |
| geo          | 8089     | 8090       |
| user         | 8091     | 8092       |
| rate         | 8093     | 8094       |
| profile      | 8095     | 8096       |
| search       | 8097     | (none)     |
| **review**   | **8098** | **8099**   |

## Data model

Seed data at `data/review-seed.json`:
```json
[
  {
    "reviewId": "1",
    "hotelId": "1",
    "name": "Person 1",
    "rating": 3.4,
    "description": "...",
    "images": { "url": "some url", "default": false }
  }
]
```

Note: `images` is a **single object** in the seed data and original proto (not a list). The WIT
field will be named `image` (singular) to avoid confusion, while the proto field stays `images`
to match the original.

## gRPC proto files to create

### `proto/review-store.proto`

```proto
syntax = "proto3";
package review_store;
service ReviewStore {
  rpc Init(InitRequest) returns (InitResponse);
  rpc LoadReviews(LoadReviewsRequest) returns (LoadReviewsResponse);
}
message InitRequest {}
message InitResponse {}
message LoadReviewsRequest {}
message LoadReviewsResponse { repeated Review reviews = 1; }
message Review {
  string reviewId    = 1;
  string hotelId     = 2;
  string name        = 3;
  float  rating      = 4;
  string description = 5;
  Image  image       = 6;
}
message Image { string url = 1; bool default = 2; }
```

### `proto/review.proto`

```proto
syntax = "proto3";
package review;
service Review {
  rpc GetReviews(Request) returns (Result);
}
message Request { string hotelId = 1; }
message Result { repeated ReviewComm reviews = 1; }
message ReviewComm {
  string reviewId    = 1;
  string hotelId     = 2;
  string name        = 3;
  float  rating      = 4;
  string description = 5;
  Image  images      = 6;
}
message Image { string url = 1; bool default = 2; }
```

Note: `ReviewComm.images` keeps the original proto field name (singular `Image` message, named
`images`). In Rust this becomes `images: Option<Image>`.

## Files to create

### 1. `components/review_store/wit/review-store.wit`

```wit
package hotel:review-data;

interface review-store {
  record image {
    url:     string,
    default: bool,
  }

  record review {
    review-id:   string,
    hotel-id:    string,
    name:        string,
    rating:      f32,
    description: string,
    image:       image,
  }

  init:         func();
  load-reviews: func() -> list<review>;
}

world review-store-world {
  include wasi:cli/imports@0.2.0;
  import host:storage/collection;
  export review-store;
}

world review-store-host-world {
  import host:storage/collection;
  export review-store;
}
```

### 2. `components/review_store/wkg.toml`

```toml
[overrides]
"host:storage" = { path = "../../wit/host-storage" }
```

Run `wkg wit fetch` inside `components/review_store/` to generate `wkg.lock`.

### 3. `components/review_store/main.go`

Run `wit-bindgen-go generate --world review-store-world --out ./components/review_store ./components/review_store/wit`
first, then check the generated `*.wit.go` for actual field names (`ReviewID` vs `ReviewId`, etc.)
before writing this file.

```go
package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	col      "hotel-components/components/review_store/host/storage/collection"
	revstore "hotel-components/components/review_store/hotel/review-data/review-store"
)

type seedImage struct {
	Url     string `json:"url"`
	Default bool   `json:"default"`
}

type seedReview struct {
	ReviewId    string    `json:"reviewId"`
	HotelId     string    `json:"hotelId"`
	Name        string    `json:"name"`
	Rating      float32   `json:"rating"`
	Description string    `json:"description"`
	Images      seedImage `json:"images"`
}

var (
	reviews   []revstore.Review
	allLoaded bool
)

func main() {}

func init() {
	revstore.Exports.Init        = doInit
	revstore.Exports.LoadReviews = loadReviews
}

func doInit() {
	if col.Count() > 0 {
		return
	}
	data, err := os.ReadFile("/data/review-seed.json")
	if err != nil {
		panic("read seed file: " + err.Error())
	}
	var seeds []seedReview
	if err := json.Unmarshal(data, &seeds); err != nil {
		panic("parse seed data: " + err.Error())
	}
	docs := make([]col.Document, len(seeds))
	for i, s := range seeds {
		b, _ := json.Marshal(s)
		docs[i] = col.Document(cm.ToList(b))
	}
	col.InsertMany(cm.ToList(docs))
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	rawDocs := col.FindAll().Slice()
	for _, raw := range rawDocs {
		var s seedReview
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		reviews = append(reviews, revstore.Review{
			ReviewID:    s.ReviewId,    // adjust to match bindgen output
			HotelID:     s.HotelId,    // adjust to match bindgen output
			Name:        s.Name,
			Rating:      s.Rating,
			Description: s.Description,
			Image: revstore.Image{
				URL:     s.Images.Url,   // adjust to match bindgen output
				Default: s.Images.Default,
			},
		})
	}
	allLoaded = true
}

func loadReviews() cm.List[revstore.Review] {
	ensureLoaded()
	return cm.ToList(reviews)
}
```

### 4. `components/review/wit/review.wit`

```wit
package hotel:review;

interface review {
  record image {
    url:     string,
    default: bool,
  }

  record review-comm {
    review-id:   string,
    hotel-id:    string,
    name:        string,
    rating:      f32,
    description: string,
    image:       image,
  }

  init:        func();
  get-reviews: func(hotel-id: string) -> list<review-comm>;
}

world review-world {
  include wasi:cli/imports@0.2.0;
  import hotel:review-data/review-store;
  import host:cache/keyvalue;
  export review;
}

world review-host-world {
  import hotel:review-data/review-store;
  import host:cache/keyvalue;
  export review;
}

world review-composed-host-world {
  import host:storage/collection;
  import host:cache/keyvalue;
  export review;
}
```

### 5. `components/review/wkg.toml`

```toml
[overrides]
"hotel:review-data" = { path = "../review_store/wit" }
"host:storage"      = { path = "../../wit/host-storage" }
"host:cache"        = { path = "../../wit/host-cache" }
```

Run `wkg wit fetch` inside `components/review/` to generate `wkg.lock`.

### 6. `components/review/service.go`

Pure Go business logic — no WIT types, no imports beyond `encoding/json`:

```go
package main

import "encoding/json"

type Image struct {
	Url     string
	Default bool
}

type Review struct {
	ReviewId    string
	HotelId     string
	Name        string
	Rating      float32
	Description string
	Image       Image
}

type Service struct{}

func NewService() *Service { return &Service{} }

func (s *Service) GetReviews(
	hotelId string,
	loadAll  func() []Review,
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []Review {
	if val, ok := cacheGet(hotelId); ok {
		var result []Review
		if err := json.Unmarshal(val, &result); err == nil {
			return result
		}
	}

	var result []Review
	for _, r := range loadAll() {
		if r.HotelId == hotelId {
			result = append(result, r)
		}
	}

	if b, err := json.Marshal(result); err == nil {
		cacheSet(hotelId, b)
	}
	return result
}
```

### 7. `components/review/main.go`

Run `wit-bindgen-go generate --world review-world --out ./components/review ./components/review/wit`
first and check generated field names before writing.

```go
package main

import (
	"go.bytecodealliance.org/cm"

	kv     "hotel-components/components/review/host/cache/keyvalue"
	store  "hotel-components/components/review/hotel/review-data/review-store"
	revapi "hotel-components/components/review/hotel/review/review"
)

var svc = NewService()

func main() {}

func init() {
	revapi.Exports.Init       = doInit
	revapi.Exports.GetReviews = getReviews
}

func doInit() {
	store.Init()
}

func getReviews(hotelId string) cm.List[revapi.ReviewComm] {
	reviews := svc.GetReviews(hotelId, loadAll, cacheGet, cacheSet)
	witResult := make([]revapi.ReviewComm, len(reviews))
	for i, r := range reviews {
		witResult[i] = revapi.ReviewComm{
			ReviewID:    r.ReviewId,    // adjust to match bindgen output
			HotelID:     r.HotelId,    // adjust to match bindgen output
			Name:        r.Name,
			Rating:      r.Rating,
			Description: r.Description,
			Image:       revapi.Image{URL: r.Image.Url, Default: r.Image.Default},
		}
	}
	return cm.ToList(witResult)
}

func loadAll() []Review {
	witRevs := store.LoadReviews().Slice()
	result := make([]Review, len(witRevs))
	for i, wr := range witRevs {
		result[i] = Review{
			ReviewId:    string([]byte(wr.ReviewID)),    // adjust to match bindgen output
			HotelId:     string([]byte(wr.HotelID)),    // adjust to match bindgen output
			Name:        string([]byte(wr.Name)),
			Rating:      wr.Rating,
			Description: string([]byte(wr.Description)),
			Image:       Image{Url: string([]byte(wr.Image.URL)), Default: wr.Image.Default},
		}
	}
	return result
}

func cacheGet(key string) ([]byte, bool) {
	opt := kv.Get(key)
	if opt.None() {
		return nil, false
	}
	return opt.Some().Slice(), true
}

func cacheSet(key string, val []byte) {
	kv.Set(key, cm.ToList(val))
}
```

### 8. `hosts/review/Cargo.toml`

```toml
[package]
name = "review-host"
version = "0.1.0"
edition = "2021"

[dependencies]
host-lib      = { path = "../../host-lib" }
wasmtime      = { version = "25", features = ["component-model", "async"] }
wasmtime-wasi = "25"
mongodb       = "3"
bson          = "2"
tonic         = { version = "0.12", features = ["transport"] }
prost         = "0.13"
async-trait   = "0.1"
tokio         = { version = "1", features = ["full"] }
anyhow        = "1"

[build-dependencies]
tonic-build = "0.12"
```

### 9. `hosts/review/build.rs`

Copy verbatim from any other host — compiles every `.proto` in `../../proto/`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = std::path::Path::new("../../proto");
    println!("cargo:rerun-if-changed={}", proto_dir.display());

    let protos: Vec<_> = std::fs::read_dir(proto_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("proto"))
        .collect();

    tonic_build::configure().compile_protos(&protos, &[proto_dir])?;

    Ok(())
}
```

### 10. `hosts/review/src/main.rs`

```rust
mod grpc;
mod store;
mod svc;
mod abi;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mode = std::env::var("MODE").unwrap_or_else(|_| "abi".into());
    match mode.as_str() {
        "store" => store::run().await,
        "svc"   => svc::run().await,
        "abi"   => abi::run().await,
        other   => anyhow::bail!("unknown MODE={other}; expected store|svc|abi"),
    }
}
```

### 11. `hosts/review/src/grpc.rs`

```rust
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
```

### 12. `hosts/review/src/store.rs`

The accessor method names (`hotel_review_data_review_store`) are derived from the WIT package path
`hotel:review-data/review-store`. Run `cargo check` after the first write and the compiler will
tell you the correct name if it's wrong.

```rust
use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("review_store");
}
use proto::{
    review_store_server::{ReviewStore, ReviewStoreServer},
    Review as ProtoReview, Image as ProtoImage,
    InitRequest, InitResponse, LoadReviewsRequest, LoadReviewsResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/review_store/wit",
    world: "review-store-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(ReviewStoreHostWorld);

#[tonic::async_trait]
impl ReviewStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_review_data_review_store()
            .call_init(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_reviews(
        &self,
        _req: Request<LoadReviewsRequest>,
    ) -> Result<Response<LoadReviewsResponse>, Status> {
        let mut store = self.store.lock().await;
        let wit_reviews = self.instance
            .hotel_review_data_review_store()
            .call_load_reviews(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadReviewsResponse {
            reviews: wit_reviews.into_iter().map(|r| ProtoReview {
                review_id:   r.review_id,
                hotel_id:    r.hotel_id,
                name:        r.name,
                rating:      r.rating,
                description: r.description,
                image: Some(ProtoImage {
                    url:     r.image.url,
                    default: r.image.default,
                }),
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    ReviewStoreHostWorld, ReviewStoreServer, hotel_review_data_review_store,
    "review-db", "reviews",
    "0.0.0.0:8099", "review-store.wasm", "review-host"
);
```

### 13. `hosts/review/src/svc.rs`

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::Linker;
use crate::grpc::{ReviewComponent, ReviewComm, Image};

mod store_proto {
    tonic::include_proto!("review_store");
}
use store_proto::{review_store_client::ReviewStoreClient, InitRequest, LoadReviewsRequest};

wasmtime::component::bindgen!({
    path: "../../components/review/wit",
    world: "review-host-world",
    async: true,
});

pub struct HostData {
    pub wasi:         wasmtime_wasi::WasiCtx,
    pub table:        wasmtime_wasi::ResourceTable,
    pub store_client: Arc<Mutex<ReviewStoreClient<tonic::transport::Channel>>>,
    pub cache:        Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::review_data::review_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC review-store Init failed");
    }

    async fn load_reviews(&mut self) -> Vec<hotel::review_data::review_store::Review> {
        let resp = self.store_client.lock().await
            .load_reviews(tonic::Request::new(LoadReviewsRequest {})).await
            .expect("gRPC review-store LoadReviews failed")
            .into_inner();
        resp.reviews.into_iter().map(|r| {
            let img = r.image.unwrap_or_default();
            hotel::review_data::review_store::Review {
                review_id:   r.review_id,
                hotel_id:    r.hotel_id,
                name:        r.name,
                rating:      r.rating,
                description: r.description,
                image: hotel::review_data::review_store::Image {
                    url:     img.url,
                    default: img.default,
                },
            }
        }).collect()
    }
}

#[async_trait::async_trait]
impl host::cache::keyvalue::Host for HostData {
    async fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.cache.lock().await.get(&key).cloned()
    }

    async fn set(&mut self, key: String, value: Vec<u8>) {
        self.cache.lock().await.insert(key, value);
    }
}

#[async_trait::async_trait]
impl ReviewComponent for ReviewHostWorld {
    type Data = HostData;

    async fn get_reviews(
        &self,
        store: &mut Store<HostData>,
        hotel_id: String,
    ) -> Result<Vec<ReviewComm>> {
        let wit_reviews = self.hotel_review_review()
            .call_get_reviews(store, &hotel_id).await?;
        Ok(wit_reviews.into_iter().map(|r| ReviewComm {
            review_id:   r.review_id,
            hotel_id:    r.hotel_id,
            name:        r.name,
            rating:      r.rating,
            description: r.description,
            images: Some(Image {
                url:     r.image.url,
                default: r.image.default,
            }),
        }).collect())
    }
}

pub async fn run() -> anyhow::Result<()> {
    use wasmtime::component::Component;

    let store_addr = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8099".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8098".into())
        .parse()?;
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "review.wasm".into());

    let store_client = ReviewStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReviewHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi:         host_lib::make_wasi_ctx(),
        table:        wasmtime_wasi::ResourceTable::new(),
        store_client,
        cache,
    };
    let mut store = wasmtime::Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance = ReviewHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_review_review().call_init(&mut store).await?;

    println!("review-host [svc] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
```

### 14. `hosts/review/src/abi.rs`

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::{Component, Linker};
use crate::grpc::{ReviewComponent, ReviewComm, Image};

wasmtime::component::bindgen!({
    path: "../../components/review/wit",
    world: "review-composed-host-world",
    async: true,
});

pub struct AbiData {
    pub wasi:       wasmtime_wasi::WasiCtx,
    pub table:      wasmtime_wasi::ResourceTable,
    pub collection: Arc<mongodb::Collection<bson::Document>>,
    pub cache:      Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl wasmtime_wasi::WasiView for AbiData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

host_lib::impl_collection_host!(AbiData);

#[async_trait::async_trait]
impl host::cache::keyvalue::Host for AbiData {
    async fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.cache.lock().await.get(&key).cloned()
    }

    async fn set(&mut self, key: String, value: Vec<u8>) {
        self.cache.lock().await.insert(key, value);
    }
}

#[async_trait::async_trait]
impl ReviewComponent for ReviewComposedHostWorld {
    type Data = AbiData;

    async fn get_reviews(
        &self,
        store: &mut Store<AbiData>,
        hotel_id: String,
    ) -> Result<Vec<ReviewComm>> {
        let wit_reviews = self.hotel_review_review()
            .call_get_reviews(store, &hotel_id).await?;
        Ok(wit_reviews.into_iter().map(|r| ReviewComm {
            review_id:   r.review_id,
            hotel_id:    r.hotel_id,
            name:        r.name,
            rating:      r.rating,
            description: r.description,
            images: Some(Image {
                url:     r.image.url,
                default: r.image.default,
            }),
        }).collect())
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8098".into())
        .parse()?;
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "review-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(mongo.database("review-db").collection("reviews"));
    let cache: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<AbiData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReviewComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = AbiData {
        wasi:       host_lib::make_store_wasi_ctx(&data_dir)?,
        table:      wasmtime_wasi::ResourceTable::new(),
        collection,
        cache,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance = ReviewComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_review_review().call_init(&mut store).await?;

    println!("review-host [abi] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
```

## Files to modify

### `Cargo.toml` (workspace root)

Add `"hosts/review"` to the `members` array.

### `Makefile`

Add `review` to the `SERVICES` variable (it has a store companion, so it gets the full treatment):

```makefile
SERVICES := recommendation attractions geo user rate profile review
```

This automatically gets `review` included in `all`, `all-composed`, `wit-deps`, and `generate`
via the existing foreach loops. No other Makefile changes needed.

### `docker-compose.tcp.yml`

Add before `volumes:`:

```yaml
  mongodb-review:
    image: mongo:5.0
    volumes:
      - review-data:/data/db

  review-store:
    build:
      context: .
      target: runtime
      args:
        SERVICE: review
    environment:
      MODE: store
      MONGO_URI: "mongodb://mongodb-review:27017"
      LISTEN_ADDR: "0.0.0.0:8099"
      DATA_DIR: /data
      WASM_FILE: /app/review-store.wasm
    ports:
      - "8099:8099"
    volumes:
      - ./data/review-seed.json:/data/review-seed.json:ro
    depends_on:
      - mongodb-review
    restart: on-failure

  review:
    build:
      context: .
      target: runtime
      args:
        SERVICE: review
    environment:
      MODE: svc
      STORE_ADDR: "http://review-store:8099"
      LISTEN_ADDR: "0.0.0.0:8098"
      WASM_FILE: /app/review.wasm
    ports:
      - "8098:8098"
    depends_on:
      - review-store
    restart: on-failure
```

Also add `review-data:` under `volumes:`.

### `docker-compose.abi.yml`

Add before `volumes:`:

```yaml
  mongodb-review:
    image: mongo:5.0
    volumes:
      - review-data:/data/db

  review:
    build:
      context: .
      target: runtime
      args:
        SERVICE: review
    environment:
      MODE: abi
      MONGO_URI: "mongodb://mongodb-review:27017"
      LISTEN_ADDR: "0.0.0.0:8098"
      DATA_DIR: /data
      WASM_FILE: /app/review-composed.wasm
    ports:
      - "8098:8098"
    volumes:
      - ./data/review-seed.json:/data/review-seed.json:ro
    depends_on:
      - mongodb-review
    restart: on-failure
```

Also add `review-data:` under `volumes:`.

## Bindgen order of operations

Run these commands in order, and verify the generated field names before writing any `.go` files:

```bash
# Store component
cd components/review_store && wkg wit fetch && cd ../..
wit-bindgen-go generate --world review-store-world \
  --out ./components/review_store ./components/review_store/wit

# Check field names in generated output:
grep -E "type Review|ReviewID|ReviewId|HotelID|HotelId|Image|URL|Url" \
  components/review_store/hotel/review-data/review-store/review-store.wit.go

# Main component
cd components/review && wkg wit fetch && cd ../..
wit-bindgen-go generate --world review-world \
  --out ./components/review ./components/review/wit

# Check field names in generated output:
grep -E "type ReviewComm|ReviewID|HotelID|Image|URL" \
  components/review/hotel/review/review/review.wit.go
```

## Common pitfalls

- **Go field names**: `review-id` in WIT → bindgen typically generates `ReviewID` (not `ReviewId`).
  Always grep the generated files before writing Go code.
- **Rust field names**: kebab-case WIT → snake_case Rust. `review-id` → `review_id`,
  `hotel-id` → `hotel_id`. The WIT field `image` (already snake_case) stays `image`.
- **Accessor method names in Rust**: 
  - `hotel:review-data/review-store` export → accessor `hotel_review_data_review_store()`
  - `hotel:review/review` export → accessor `hotel_review_review()`
  Run `cargo check` — the compiler will show the correct name if mismatched.
- **`images` vs `image`**: The proto field `ReviewComm.images` is the original's name; the WIT
  field is `review-comm.image`. When mapping from WIT to proto in `svc.rs`/`abi.rs`, the WIT
  side is `r.image` and the proto side is `images: Some(Image {...})`.
- **`f32` rating**: The `rating` field is `f32` in both WIT and proto (proto `float` = 32-bit).
  No cast needed when passing from Rust proto → WIT or back.
- **`run_store!` macro**: Requires `define_store_service!(ReviewStoreHostWorld)` and the gRPC
  trait impl to be in scope before the macro call (see `hosts/profile/src/store.rs`).

## How to test once built

```bash
# Build everything
make build/review-store.wasm build/review.wasm build/review-composed.wasm
cargo build -p review-host

# Run in svc mode (requires review-store already up on :8099)
WASM_FILE=build/review-store.wasm MODE=store ./target/debug/review-host &
WASM_FILE=build/review.wasm MODE=svc STORE_ADDR=http://localhost:8099 ./target/debug/review-host

# Test
grpcurl -plaintext \
  -import-path ./proto -proto review.proto \
  -d '{"hotelId":"1"}' \
  localhost:8098 review.Review/GetReviews
```
