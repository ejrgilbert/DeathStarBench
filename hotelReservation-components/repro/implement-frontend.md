# Task: Implement the `frontend` service

Working directory: `eval/benchmarks/msa/DeathStarBench/hotelReservation-components/`

## Context

This repo is a port of the DeathStarBench hotelReservation microservices to WebAssembly
components (TinyGo + Wasmtime). Each service is split into a TinyGo component and a Rust
host. The `frontend` is the **exception**: it has no WIT/WASM component at all. It is a
plain Rust HTTP server that aggregates calls to all downstream gRPC services.

The original Go implementation is at `../hotelReservation/services/frontend/server.go`.
Drop: Consul service discovery, OpenTracing, TLS, static file serving, the `/` route.
Keep: all HTTP handler logic and response formats exactly as-is.

## What the frontend does

It exposes HTTP endpoints and fans out to downstream gRPC services:

| Endpoint | Downstream gRPC calls |
|----------|-----------------------|
| `GET /hotels` | Search.Nearby → Reservation.CheckAvailability → Profile.GetProfiles |
| `GET /recommendations` | Recommendation.GetRecommendations → Profile.GetProfiles |
| `GET /user` | User.CheckUser |
| `GET /review` | User.CheckUser → Review.GetReviews |
| `GET /restaurants` | User.CheckUser → Attractions.NearbyRest |
| `GET /museums` | User.CheckUser → Attractions.NearbyMus |
| `GET /cinema` | User.CheckUser → Attractions.NearbyCinema |
| `GET /reservation` | User.CheckUser → Reservation.MakeReservation |

All parameters are query-string params (GET). All responses are JSON.

## Proto files (already exist at `proto/`)

```
search.proto        — Search.Nearby(NearbyRequest{lat,lon,inDate,outDate}) → SearchResult{hotelIds}
profile.proto       — Profile.GetProfiles(Request{hotelIds,locale}) → Result{hotels:[Hotel{id,name,phoneNumber,address{lat,lon,...},images}]}
recommendation.proto — Recommendation.GetRecommendations(Request{require,lat,lon}) → Result{HotelIds}
user.proto          — User.CheckUser(Request{username,password}) → Result{correct}
review.proto        — Review.GetReviews(Request{hotelId}) → Result{reviews:[ReviewComm]}
attractions.proto   — Attractions.NearbyRest/NearbyMus/NearbyCinema(Request{hotel_id}) → Result{attraction_ids}
reservation.proto   — Reservation.CheckAvailability/MakeReservation(Request{customerName,hotelId[],inDate,outDate,roomNumber}) → Result{hotelId[]}
```

Note: `recommendation.Result` has field `HotelIds` (capital H, capital I) — that's what
the proto generates as `hotel_ids` in Rust (prost lowercases it). Double-check with
`cargo check` if there are field name issues.

Note: `attractions.Request` uses `hotel_id` (snake_case with underscore) — different from
other services which use `hotelId`. Same for `Result.attraction_ids`.

## HTTP handler details

### GET /hotels
Query params: `lat` (f32), `lon` (f32), `inDate` (string), `outDate` (string), `locale` (string, default "en")

```
1. Search.Nearby(lat, lon, inDate, outDate) → searchResp.hotel_ids
2. Reservation.CheckAvailability("", searchResp.hotel_ids, inDate, outDate, roomNumber=1) → reservResp.hotel_id
3. Profile.GetProfiles(reservResp.hotel_id, locale) → profileResp.hotels
4. Return geoJSONResponse(profileResp.hotels)
```

### GET /recommendations
Query params: `lat` (f64), `lon` (f64), `require` ("dis"|"rate"|"price"), `locale` (string, default "en")

```
1. Recommendation.GetRecommendations(require, lat, lon) → recResp.hotel_ids
2. Profile.GetProfiles(recResp.hotel_ids, locale) → profileResp.hotels
3. Return geoJSONResponse(profileResp.hotels)
```

### GET /user
Query params: `username`, `password`

```
1. User.CheckUser(username, password) → resp.correct
2. Return {"message": "Login successfully!"} or {"message": "Failed. Please check your username and password. "}
```

### GET /review
Query params: `username`, `password`, `hotelId`

```
1. User.CheckUser(username, password) [result ignored in message if reviews exist]
2. Review.GetReviews(hotelId) → revResp.reviews
3. Return {"message": "Have reviews = N"} or {"message": "Failed. No Reviews. "}
```

(Note: original code checks login first but overwrites `str` with review count unconditionally)

### GET /restaurants
Query params: `username`, `password`, `hotelId`

```
1. User.CheckUser(username, password)
2. Attractions.NearbyRest(hotelId) → resp.attraction_ids
3. Return {"message": "Have restaurants = N"} or {"message": "Failed. No Restaurants. "}
```

### GET /museums
Same pattern as /restaurants but calls `Attractions.NearbyMus`.
Response: `"Have museums = N"` or `"Failed. No Museums. "`

### GET /cinema
Same pattern but `Attractions.NearbyCinema`.
Response: `"Have cinemas = N"` or `"Failed. No Cinemas. "`

### GET /reservation
Query params: `inDate`, `outDate`, `hotelId`, `customerName`, `username`, `password`, `number` (int, default 0)

Validate date format (YYYY-MM-DD: exactly 10 chars, digits at positions 0-3,5-6,8-9, dashes at 4,7).

```
1. User.CheckUser(username, password) → correct
   str = "Reserve successfully!" or "Failed. Please check your username and password. "
2. Reservation.MakeReservation(customerName, [hotelId], inDate, outDate, number) → resResp
   if resResp.hotel_id is empty: str = "Failed. Already reserved. "
3. Return {"message": str}
```

Note: MakeReservation is called regardless of whether login succeeded (matches original).

## geoJSONResponse format

```json
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "id": "<hotel.id>",
      "properties": { "name": "<hotel.name>", "phone_number": "<hotel.phone_number>" },
      "geometry": { "type": "Point", "coordinates": [lon, lat] }
    }
  ]
}
```

## Environment variables

All have defaults for local testing:

| Var | Default |
|-----|---------|
| `LISTEN_ADDR` | `0.0.0.0:8080` |
| `SEARCH_ADDR` | `http://localhost:8097` |
| `PROFILE_ADDR` | `http://localhost:8095` |
| `RECOMMENDATION_ADDR` | `http://localhost:8085` |
| `USER_ADDR` | `http://localhost:8091` |
| `REVIEW_ADDR` | `http://localhost:8098` |
| `ATTRACTIONS_ADDR` | `http://localhost:8087` |
| `RESERVATION_ADDR` | `http://localhost:8100` |

## Port assignments summary (for context)

| Service | Port |
|---------|------|
| recommendation | 8085 |
| attractions | 8087 |
| geo | 8089 |
| user | 8091 |
| rate | 8093 |
| profile | 8095 |
| search | 8097 |
| review | 8098 |
| reservation | 8100 |
| **frontend** | **8080** |

## Files to create

### `hosts/frontend/Cargo.toml`

```toml
[package]
name = "frontend-host"
version = "0.1.0"
edition = "2021"

[dependencies]
tonic         = { version = "0.12", features = ["transport"] }
prost         = "0.13"
tokio         = { version = "1", features = ["full"] }
axum          = "0.7"
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
anyhow        = "1"

[build-dependencies]
tonic-build = "0.12"
```

### `hosts/frontend/build.rs`

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

### `hosts/frontend/src/main.rs`

All logic goes in `main.rs` (no need to split into modules — there's no component/linker
boilerplate, just HTTP handlers and gRPC clients).

Structure:
- Define a `State` struct holding all 7 gRPC clients (Arc-wrapped for axum sharing)
- `#[tokio::main] async fn main()` connects all clients from env vars, builds axum Router,
  listens on `LISTEN_ADDR`
- One async handler function per endpoint

Use `axum::extract::{Query, State}` for params. Return `axum::response::Json` or
`(StatusCode, String)` for errors.

**Use `Arc<Mutex<Client>>` only if needed — tonic clients are `Clone` and cheap to clone,
so you can just store them directly in State and derive Clone, or wrap State in Arc.**

## Files to modify

### `Cargo.toml` (workspace root)

Add `"hosts/frontend"` to the `members` array.

### `docker-compose.tcp.yml`

Add before `volumes:`:

```yaml
  frontend:
    build:
      context: .
      target: runtime
      args:
        SERVICE: frontend
    environment:
      LISTEN_ADDR:         "0.0.0.0:8080"
      SEARCH_ADDR:         "http://search:8097"
      PROFILE_ADDR:        "http://profile:8095"
      RECOMMENDATION_ADDR: "http://recommendation:8085"
      USER_ADDR:           "http://user:8091"
      REVIEW_ADDR:         "http://review:8098"
      ATTRACTIONS_ADDR:    "http://attractions:8087"
      RESERVATION_ADDR:    "http://reservation:8100"
    ports:
      - "8080:8080"
    depends_on:
      - search
      - profile
      - recommendation
      - user
      - review
      - attractions
      - reservation
    restart: on-failure
```

### `docker-compose.abi.yml`

Same frontend block, but downstream service names are identical (they use the same
container names in both configs), so the env vars are the same.

## Important notes

- The frontend has **no WASM component** — do not add it to the `SERVICES` variable in the
  Makefile (that variable drives the TinyGo build rules).
- Tonic clients are `Clone`: store them in a plain struct and `#[derive(Clone)]`, then
  use `axum::extract::State<AppState>` where `AppState` is that struct wrapped in `Arc`.
- The `build.rs` compiles ALL protos in `../../proto/` which includes store protos — that's
  fine, the unused generated code is just dead code.
- Run `cargo check -p frontend-host` after writing and fix any field name mismatches
  (prost snake_cases proto field names: `HotelIds` → `hotel_ids`, `hotelId` → `hotel_id`,
  `phoneNumber` → `phone_number`, `attraction_ids` stays `attraction_ids`, etc.).
- `axum` 0.7 uses `axum::Router::with_state(state)` pattern; handlers take
  `State(state): State<AppState>` extractor.
- For error responses use `(StatusCode::BAD_REQUEST, msg.to_string())` — axum accepts
  tuples as responses.

## How to test once built

```bash
cargo build -p frontend-host

# With all services already running:
curl "http://localhost:8080/hotels?lat=37.7749&lon=-122.4194&inDate=2015-04-09&outDate=2015-04-10"
curl "http://localhost:8080/user?username=Cornell_0&password=1111111111"
curl "http://localhost:8080/reservation?inDate=2015-04-09&outDate=2015-04-10&hotelId=1&customerName=test&username=Cornell_0&password=1111111111&number=1"
```
