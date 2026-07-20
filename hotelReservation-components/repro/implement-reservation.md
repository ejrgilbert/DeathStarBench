# Task: Implement the `reservation` service

Working directory: `eval/benchmarks/msa/DeathStarBench/hotelReservation-components/`

## Context

This repo is a port of the DeathStarBench hotelReservation microservices to WebAssembly components
(TinyGo + Wasmtime). Each service is split into:
- A TinyGo **component** (`components/<svc>/`) that holds business logic
- A Rust **host** (`hosts/<svc>/`) that embeds the component in Wasmtime and serves gRPC
- Most services also have a **store** component (`components/<svc>_store/`) that owns MongoDB access

The `reservation` service follows the standard store/svc/abi triple-mode pattern used by `profile`
and `rate`, but it is **significantly more complex** than those services because:

1. It uses **two MongoDB collections**: `number` (hotel capacity, static) and `reservation`
   (bookings, written at runtime)
2. It has **two RPCs**: `CheckAvailability` and `MakeReservation`
3. `MakeReservation` **writes** to MongoDB — no other ported service does this
4. The two collections require **two custom WIT collection interfaces** defined locally
   (not the shared `host:storage/collection`), so the `impl_collection_host!` and
   `define_store_service!` macros from `host-lib` do **not** apply to the store host
5. You must add `mongo_insert_one` to `host-lib/src/lib.rs` (it doesn't exist yet)

The original Go implementation is at `../hotelReservation/services/reservation/server.go`.
In our port, memcache is replaced by `host:cache/keyvalue` (in-memory HashMap), and MongoDB
access goes via WIT host interfaces. The concurrency and tracing are dropped.

## Important rules

- Every string coming back from a WIT import call **must** be copied with `string([]byte(s))`
  before storing or passing onward. See `components/rate/main.go` for examples.
- WIT export function parameters (e.g. `hotelIds` list, `inDate` string) are also passed via
  WIT-allocated buffers and should be copied before long-term storage.
- Run `wit-bindgen-go generate` and **grep the generated files** for field names before writing
  any Go code — never guess them.
- Run `cargo check -p reservation-host` after writing Rust and fix any accessor name mismatches
  (the compiler will show the correct name).

## Reference implementations

Study these before implementing — reservation deviates from them in specific ways noted below:
- `components/profile_store/main.go` — seed-load + MongoDB pattern via collection WIT import
- `components/profile/service.go` — pure-Go business logic
- `components/profile/main.go` — WIT glue + string copy pattern
- `hosts/profile/src/grpc.rs` — gRPC service trait
- `hosts/profile/src/store.rs` — store host (reservation's store.rs is **custom**, not macro-based)
- `hosts/profile/src/svc.rs` — svc host with store + cache client
- `hosts/profile/src/abi.rs` — abi host with direct MongoDB + cache
- `host-lib/src/lib.rs` — macros and helper functions

## Port assignments

| Service      | svc port | store port |
|--------------|----------|------------|
| rate         | 8093     | 8094       |
| profile      | 8095     | 8096       |
| search       | 8097     | (none)     |
| review       | 8098     | 8099       |
| **reservation** | **8100** | **8101** |

## Data model

Two seed files (already exist in `data/`):

**`data/reservation-numbers-seed.json`** — hotel capacity:
```json
[{"hotelId": "1", "numberOfRoom": 200}, {"hotelId": "2", "numberOfRoom": 200}, ...]
```

**`data/reservation-reservations-seed.json`** — existing bookings:
```json
[{"hotelId": "4", "customerName": "Alice", "inDate": "2015-04-09", "outDate": "2015-04-10", "number": 1}]
```

The `reservation` collection stores one document **per day** in the range: `inDate` and `outDate`
are the start and end of a single one-day slot. `MakeReservation` for a multi-day stay inserts
one document per day.

## The two-collection WIT design

Because `host:storage/collection` is a single-collection abstraction, and the store component
needs **two** collections, define two lightweight collection interfaces **inside the store's own
WIT package** (`hotel:reservation-data`). The Rust store host implements them directly against
MongoDB — no `impl_collection_host!` macro needed.

This means:
- `reservation_store/wit/reservation-store.wit` defines `numbers-col`, `reservations-col`, and
  `reservation-store` **all in the same file / same package**
- The `wkg.toml` for `reservation_store` needs no overrides (no external package dependencies
  other than WASI, which is fetched automatically)
- The `wkg.toml` for `reservation` (svc) only overrides `hotel:reservation-data` and `host:cache`

---

## Step 0: Add `mongo_insert_one` to `host-lib/src/lib.rs`

Add this function alongside the existing `mongo_insert_many`:

```rust
pub async fn mongo_insert_one(
    collection: &Arc<mongodb::Collection<bson::Document>>,
    doc: Vec<u8>,
) -> Result<()> {
    let bson_doc = json_to_bson(&doc)?;
    collection.insert_one(bson_doc).await?;
    Ok(())
}
```

---

## Files to create

### 1. `proto/reservation-store.proto`

```proto
syntax = "proto3";
package reservation_store;

service ReservationStore {
  rpc Init(InitRequest)                       returns (InitResponse);
  rpc LoadNumbers(LoadNumbersRequest)         returns (LoadNumbersResponse);
  rpc LoadReservations(LoadReservationsRequest) returns (LoadReservationsResponse);
  rpc InsertReservation(InsertReservationRequest) returns (InsertReservationResponse);
}

message InitRequest  {}
message InitResponse {}
message LoadNumbersRequest      {}
message LoadReservationsRequest {}
message InsertReservationRequest {
  string hotelId      = 1;
  string customerName = 2;
  string inDate       = 3;
  string outDate      = 4;
  uint32 number       = 5;
}
message InsertReservationResponse {}

message NumberRec {
  string hotelId       = 1;
  uint32 numberOfRoom  = 2;
}
message ReservationRec {
  string hotelId       = 1;
  string customerName  = 2;
  string inDate        = 3;
  string outDate       = 4;
  uint32 number        = 5;
}
message LoadNumbersResponse      { repeated NumberRec      numbers      = 1; }
message LoadReservationsResponse { repeated ReservationRec reservations = 1; }
```

### 2. `proto/reservation.proto`

Keep field names identical to the original proto so existing clients work:

```proto
syntax = "proto3";
package reservation;

service Reservation {
  rpc CheckAvailability(Request) returns (Result);
  rpc MakeReservation(Request)   returns (Result);
}

message Request {
  string          customerName = 1;
  repeated string hotelId      = 2;
  string          inDate       = 3;
  string          outDate      = 4;
  int32           roomNumber   = 5;
}
message Result {
  repeated string hotelId = 1;
}
```

### 3. `components/reservation_store/wit/reservation-store.wit`

```wit
package hotel:reservation-data;

// Lightweight collection interface for the 'number' (capacity) MongoDB collection.
// Only the operations the Go store component actually needs.
interface numbers-col {
  type document = list<u8>;
  count:       func() -> u64;
  find-all:    func() -> list<document>;
  insert-many: func(docs: list<document>);
}

// Lightweight collection interface for the 'reservation' MongoDB collection.
// find-all for reads; insert-one for the write path.
interface reservations-col {
  type document = list<u8>;
  find-all:   func() -> list<document>;
  insert-one: func(doc: document);
}

interface reservation-store {
  record number-rec {
    hotel-id:       string,
    number-of-room: u32,
  }

  record reservation-rec {
    hotel-id:       string,
    customer-name:  string,
    in-date:        string,
    out-date:       string,
    number:         u32,
  }

  init:               func();
  load-numbers:       func() -> list<number-rec>;
  load-reservations:  func() -> list<reservation-rec>;
  insert-reservation: func(r: reservation-rec);
}

world reservation-store-world {
  include wasi:cli/imports@0.2.0;
  import hotel:reservation-data/numbers-col;
  import hotel:reservation-data/reservations-col;
  export reservation-store;
}

world reservation-store-host-world {
  import hotel:reservation-data/numbers-col;
  import hotel:reservation-data/reservations-col;
  export reservation-store;
}
```

### 4. `components/reservation_store/wkg.toml`

All interfaces are local to the package; WASI is fetched from the registry automatically:

```toml
[overrides]
```

(empty overrides section; the file must exist for `wkg wit fetch` to run)

### 5. `components/reservation/wit/reservation.wit`

```wit
package hotel:reservation;

interface reservation {
  init: func();
  check-availability: func(
    hotel-ids:   list<string>,
    in-date:     string,
    out-date:    string,
    room-number: s32,
  ) -> list<string>;
  make-reservation: func(
    hotel-id:      string,
    customer-name: string,
    in-date:       string,
    out-date:      string,
    room-number:   s32,
  ) -> list<string>;
}

world reservation-world {
  include wasi:cli/imports@0.2.0;
  import hotel:reservation-data/reservation-store;
  import host:cache/keyvalue;
  export reservation;
}

world reservation-host-world {
  import hotel:reservation-data/reservation-store;
  import host:cache/keyvalue;
  export reservation;
}

// In composed mode the svc imports the two low-level collection interfaces directly
// (no separate store process).
world reservation-composed-host-world {
  import hotel:reservation-data/numbers-col;
  import hotel:reservation-data/reservations-col;
  import host:cache/keyvalue;
  export reservation;
}
```

### 6. `components/reservation/wkg.toml`

```toml
[overrides]
"hotel:reservation-data" = { path = "../reservation_store/wit" }
"host:cache"             = { path = "../../wit/host-cache" }
```

---

## Bindgen order of operations

```bash
# Store component
cd components/reservation_store && wkg wit fetch && cd ../..
wit-bindgen-go generate --world reservation-store-world \
  --out ./components/reservation_store ./components/reservation_store/wit

# Grep field names before writing Go:
grep -E "type NumberRec|type ReservationRec|HotelID|HotelId|NumberOfRoom|CustomerName|InDate|OutDate" \
  components/reservation_store/hotel/reservation-data/reservation-store/reservation-store.wit.go

# Svc component
cd components/reservation && wkg wit fetch && cd ../..
wit-bindgen-go generate --world reservation-world \
  --out ./components/reservation ./components/reservation/wit

# Grep field names:
grep -E "HotelID|HotelIds|InDate|OutDate|RoomNumber|CheckAvailability|MakeReservation" \
  components/reservation/hotel/reservation/reservation/reservation.wit.go
```

Expected bindgen output (verify before writing Go):
- `number-rec` → `NumberRec{ HotelID string, NumberOfRoom uint32 }`
- `reservation-rec` → `ReservationRec{ HotelID, CustomerName, InDate, OutDate string; Number uint32 }`
- `check-availability` → `Exports.CheckAvailability func(hotelIds cm.List[string], inDate, outDate string, roomNumber int32) cm.List[string]`
- `make-reservation` → `Exports.MakeReservation func(hotelId, customerName, inDate, outDate string, roomNumber int32) cm.List[string]`

---

### 7. `components/reservation_store/main.go`

```go
package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	ncol     "hotel-components/components/reservation_store/hotel/reservation-data/numbers-col"
	rcol     "hotel-components/components/reservation_store/hotel/reservation-data/reservations-col"
	revstore "hotel-components/components/reservation_store/hotel/reservation-data/reservation-store"
)

type seedNumber struct {
	HotelId      string `json:"hotelId"`
	NumberOfRoom uint32 `json:"numberOfRoom"`
}

type seedReservation struct {
	HotelId      string `json:"hotelId"`
	CustomerName string `json:"customerName"`
	InDate       string `json:"inDate"`
	OutDate      string `json:"outDate"`
	Number       uint32 `json:"number"`
}

var (
	numbers      []revstore.NumberRec
	reservations []revstore.ReservationRec
	numsLoaded   bool
	resLoaded    bool
)

func main() {}

func init() {
	revstore.Exports.Init               = doInit
	revstore.Exports.LoadNumbers        = loadNumbers
	revstore.Exports.LoadReservations   = loadReservations
	revstore.Exports.InsertReservation  = doInsertReservation
}

func doInit() {
	// Seed the numbers (capacity) collection
	if ncol.Count() == 0 {
		data, err := os.ReadFile("/data/reservation-numbers-seed.json")
		if err != nil {
			panic("read numbers seed: " + err.Error())
		}
		var seeds []seedNumber
		if err := json.Unmarshal(data, &seeds); err != nil {
			panic("parse numbers seed: " + err.Error())
		}
		docs := make([]ncol.Document, len(seeds))
		for i, s := range seeds {
			b, _ := json.Marshal(s)
			docs[i] = ncol.Document(cm.ToList(b))
		}
		ncol.InsertMany(cm.ToList(docs))
	}

	// Seed the reservations collection (always seed; find-all result tells us if empty)
	// Use a separate sentinel: try to load and only seed if empty
	existing := rcol.FindAll().Slice()
	if len(existing) == 0 {
		data, err := os.ReadFile("/data/reservation-reservations-seed.json")
		if err != nil {
			panic("read reservations seed: " + err.Error())
		}
		var seeds []seedReservation
		if err := json.Unmarshal(data, &seeds); err != nil {
			panic("parse reservations seed: " + err.Error())
		}
		for _, s := range seeds {
			b, _ := json.Marshal(s)
			rcol.InsertOne(rcol.Document(cm.ToList(b)))
		}
	}
}

func ensureNumbers() {
	if numsLoaded {
		return
	}
	rawDocs := ncol.FindAll().Slice()
	for _, raw := range rawDocs {
		var s seedNumber
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		numbers = append(numbers, revstore.NumberRec{
			HotelID:      s.HotelId,      // adjust to match bindgen output
			NumberOfRoom: s.NumberOfRoom,
		})
	}
	numsLoaded = true
}

func ensureReservations() {
	if resLoaded {
		return
	}
	rawDocs := rcol.FindAll().Slice()
	for _, raw := range rawDocs {
		var s seedReservation
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		reservations = append(reservations, revstore.ReservationRec{
			HotelID:      s.HotelId,
			CustomerName: s.CustomerName,
			InDate:       s.InDate,
			OutDate:      s.OutDate,
			Number:       s.Number,
		})
	}
	resLoaded = true
}

func loadNumbers() cm.List[revstore.NumberRec] {
	ensureNumbers()
	return cm.ToList(numbers)
}

func loadReservations() cm.List[revstore.ReservationRec] {
	ensureReservations()
	return cm.ToList(reservations)
}

func doInsertReservation(r revstore.ReservationRec) {
	s := seedReservation{
		HotelId:      r.HotelID,      // adjust to match bindgen output
		CustomerName: r.CustomerName,
		InDate:       r.InDate,
		OutDate:      r.OutDate,
		Number:       r.Number,
	}
	b, _ := json.Marshal(s)
	rcol.InsertOne(rcol.Document(cm.ToList(b)))
	// Update in-memory slice so subsequent loadReservations sees the new record
	reservations = append(reservations, r)
}
```

### 8. `components/reservation/service.go`

Pure Go — no WIT types, no imports beyond `encoding/json`, `strconv`, `time`:

```go
package main

import (
	"strconv"
	"time"
)

const dateFmt = "2006-01-02"

type NumberRec struct {
	HotelId      string
	NumberOfRoom uint32
}

type ReservationRec struct {
	HotelId      string
	CustomerName string
	InDate       string
	OutDate      string
	Number       uint32
}

// datePairs returns successive one-day [inDate, outDate) slots spanning the range.
func datePairs(inDate, outDate string) [][2]string {
	in, _ := time.Parse(dateFmt, inDate)
	out, _ := time.Parse(dateFmt, outDate)
	var pairs [][2]string
	for in.Before(out) {
		next := in.AddDate(0, 0, 1)
		pairs = append(pairs, [2]string{in.Format(dateFmt), next.Format(dateFmt)})
		in = next
	}
	return pairs
}

type Service struct{}

func NewService() *Service { return &Service{} }

// CheckAvailability returns the subset of hotelIds that can accommodate roomNumber
// rooms for every day in [inDate, outDate).
func (s *Service) CheckAvailability(
	hotelIds []string,
	inDate, outDate string,
	roomNumber int32,
	loadNumbers      func() []NumberRec,
	loadReservations func() []ReservationRec,
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []string {
	// Build capacity map, using cache to avoid repeated load.
	caps := make(map[string]int)
	var missIds []string
	for _, id := range hotelIds {
		if b, ok := cacheGet(id + "_cap"); ok {
			n, _ := strconv.Atoi(string(b))
			caps[id] = n
		} else {
			missIds = append(missIds, id)
		}
	}
	if len(missIds) > 0 {
		numMap := make(map[string]int)
		for _, nr := range loadNumbers() {
			numMap[nr.HotelId] = int(nr.NumberOfRoom)
		}
		for _, id := range missIds {
			if cap, ok := numMap[id]; ok {
				caps[id] = cap
				cacheSet(id+"_cap", []byte(strconv.Itoa(cap)))
			}
		}
	}

	// Lazily load reservations once if any date range is a cache miss.
	var resSlice []ReservationRec
	resLoaded := false

	var result []string
	for _, id := range hotelIds {
		cap := caps[id]
		available := true
		for _, pair := range datePairs(inDate, outDate) {
			cacheKey := id + "_" + pair[0] + "_" + pair[1]
			count := 0
			if b, ok := cacheGet(cacheKey); ok {
				count, _ = strconv.Atoi(string(b))
			} else {
				if !resLoaded {
					resSlice = loadReservations()
					resLoaded = true
				}
				for _, r := range resSlice {
					if r.HotelId == id && r.InDate == pair[0] && r.OutDate == pair[1] {
						count += int(r.Number)
					}
				}
				cacheSet(cacheKey, []byte(strconv.Itoa(count)))
			}
			if count+int(roomNumber) > cap {
				available = false
				break
			}
		}
		if available {
			result = append(result, id)
		}
	}
	return result
}

// MakeReservation attempts to book hotelId for the given range. Returns [hotelId]
// on success or nil if capacity is exceeded.
func (s *Service) MakeReservation(
	hotelId, customerName, inDate, outDate string,
	roomNumber int32,
	loadNumbers      func() []NumberRec,
	loadReservations func() []ReservationRec,
	insertReservation func(ReservationRec),
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []string {
	avail := s.CheckAvailability(
		[]string{hotelId}, inDate, outDate, roomNumber,
		loadNumbers, loadReservations, cacheGet, cacheSet,
	)
	if len(avail) == 0 {
		return nil
	}
	for _, pair := range datePairs(inDate, outDate) {
		insertReservation(ReservationRec{
			HotelId:      hotelId,
			CustomerName: customerName,
			InDate:       pair[0],
			OutDate:      pair[1],
			Number:       uint32(roomNumber),
		})
		// Update reservation count in cache.
		key := hotelId + "_" + pair[0] + "_" + pair[1]
		count := 0
		if b, ok := cacheGet(key); ok {
			count, _ = strconv.Atoi(string(b))
		}
		cacheSet(key, []byte(strconv.Itoa(count+int(roomNumber))))
	}
	return []string{hotelId}
}
```

### 9. `components/reservation/main.go`

Run `wit-bindgen-go generate` first and verify field names, then write:

```go
package main

import (
	"go.bytecodealliance.org/cm"

	kv     "hotel-components/components/reservation/host/cache/keyvalue"
	store  "hotel-components/components/reservation/hotel/reservation-data/reservation-store"
	resapi "hotel-components/components/reservation/hotel/reservation/reservation"
)

var svc = NewService()

func main() {}

func init() {
	resapi.Exports.Init               = doInit
	resapi.Exports.CheckAvailability  = checkAvailability
	resapi.Exports.MakeReservation    = makeReservation
}

func doInit() {
	store.Init()
}

func checkAvailability(hotelIds cm.List[string], inDate, outDate string, roomNumber int32) cm.List[string] {
	rawIds := hotelIds.Slice()
	ids := make([]string, len(rawIds))
	for i, id := range rawIds {
		ids[i] = string([]byte(id))
	}
	result := svc.CheckAvailability(
		ids,
		string([]byte(inDate)), string([]byte(outDate)),
		roomNumber,
		loadNumbers, loadReservations, cacheGet, cacheSet,
	)
	return cm.ToList(result)
}

func makeReservation(hotelId, customerName, inDate, outDate string, roomNumber int32) cm.List[string] {
	result := svc.MakeReservation(
		string([]byte(hotelId)),
		string([]byte(customerName)),
		string([]byte(inDate)),
		string([]byte(outDate)),
		roomNumber,
		loadNumbers, loadReservations, doInsertReservation, cacheGet, cacheSet,
	)
	return cm.ToList(result)
}

func loadNumbers() []NumberRec {
	witNums := store.LoadNumbers().Slice()
	result := make([]NumberRec, len(witNums))
	for i, n := range witNums {
		result[i] = NumberRec{
			HotelId:      string([]byte(n.HotelID)),   // adjust to match bindgen
			NumberOfRoom: n.NumberOfRoom,
		}
	}
	return result
}

func loadReservations() []ReservationRec {
	witRecs := store.LoadReservations().Slice()
	result := make([]ReservationRec, len(witRecs))
	for i, r := range witRecs {
		result[i] = ReservationRec{
			HotelId:      string([]byte(r.HotelID)),
			CustomerName: string([]byte(r.CustomerName)),
			InDate:       string([]byte(r.InDate)),
			OutDate:      string([]byte(r.OutDate)),
			Number:       r.Number,
		}
	}
	return result
}

func doInsertReservation(r ReservationRec) {
	store.InsertReservation(store.ReservationRec{
		HotelID:      r.HotelId,     // adjust to match bindgen
		CustomerName: r.CustomerName,
		InDate:       r.InDate,
		OutDate:      r.OutDate,
		Number:       r.Number,
	})
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

### 10. `hosts/reservation/Cargo.toml`

```toml
[package]
name = "reservation-host"
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

### 11. `hosts/reservation/build.rs`

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

### 12. `hosts/reservation/src/main.rs`

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

### 13. `hosts/reservation/src/grpc.rs`

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::Store;

mod proto {
    tonic::include_proto!("reservation");
}
use proto::reservation_server::{Reservation, ReservationServer};

#[async_trait::async_trait]
pub trait ReservationComponent: Send + Sync + 'static {
    type Data: Send + 'static;

    async fn check_availability(
        &self,
        store:       &mut Store<Self::Data>,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>>;

    async fn make_reservation(
        &self,
        store:         &mut Store<Self::Data>,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>>;
}

pub struct ReservationService<C: ReservationComponent> {
    pub store:     Arc<Mutex<Store<C::Data>>>,
    pub component: Arc<C>,
}

#[tonic::async_trait]
impl<C: ReservationComponent> Reservation for ReservationService<C> {
    async fn check_availability(
        &self,
        req: Request<proto::Request>,
    ) -> Result<Response<proto::Result>, Status> {
        let r = req.into_inner();
        let mut store = self.store.lock().await;
        let hotel_id = self.component
            .check_availability(&mut *store, r.hotel_id, r.in_date, r.out_date, r.room_number)
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
        let mut store = self.store.lock().await;
        let hotel_id = self.component
            .make_reservation(
                &mut *store,
                hotel_id_single, r.customer_name, r.in_date, r.out_date, r.room_number,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(proto::Result { hotel_id }))
    }
}

pub async fn serve<C: ReservationComponent>(
    store: Arc<Mutex<Store<C::Data>>>,
    component: Arc<C>,
    addr: SocketAddr,
) -> Result<()> {
    Server::builder()
        .add_service(ReservationServer::new(ReservationService { store, component }))
        .serve(addr)
        .await?;
    Ok(())
}
```

### 14. `hosts/reservation/src/store.rs`

The store host is **fully custom** — it cannot use `define_store_service!` or `run_store!`
because those macros assume a single-collection `StoreData`. `ReservationStoreData` has two
collection fields.

The accessor for the exported `reservation-store` interface is derived from the WIT package path
`hotel:reservation-data/reservation-store` → `hotel_reservation_data_reservation_store()`.
Run `cargo check` if it errors; the compiler will print the correct name.

```rust
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::{component::{Component, Linker}, Store};
use wasmtime_wasi::{WasiCtx, ResourceTable};

mod proto {
    tonic::include_proto!("reservation_store");
}
use proto::{
    reservation_store_server::{ReservationStore, ReservationStoreServer},
    InitRequest, InitResponse,
    LoadNumbersRequest, LoadNumbersResponse,
    LoadReservationsRequest, LoadReservationsResponse,
    InsertReservationRequest, InsertReservationResponse,
    NumberRec as ProtoNumberRec, ReservationRec as ProtoReservationRec,
};

wasmtime::component::bindgen!({
    path: "../../components/reservation_store/wit",
    world: "reservation-store-host-world",
    async: true,
});

pub struct ReservationStoreData {
    pub wasi:             WasiCtx,
    pub table:            ResourceTable,
    pub numbers_col:      Arc<mongodb::Collection<bson::Document>>,
    pub reservations_col: Arc<mongodb::Collection<bson::Document>>,
}

impl wasmtime_wasi::WasiView for ReservationStoreData {
    fn ctx(&mut self)   -> &mut WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::reservation_data::numbers_col::Host for ReservationStoreData {
    async fn count(&mut self) -> u64 {
        host_lib::mongo_count(&self.numbers_col).await.unwrap_or(0)
    }
    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        host_lib::mongo_find_all(&self.numbers_col).await.unwrap_or_default()
    }
    async fn insert_many(&mut self, docs: Vec<Vec<u8>>) {
        host_lib::mongo_insert_many(&self.numbers_col, docs).await.unwrap()
    }
}

#[async_trait::async_trait]
impl hotel::reservation_data::reservations_col::Host for ReservationStoreData {
    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        host_lib::mongo_find_all(&self.reservations_col).await.unwrap_or_default()
    }
    async fn insert_one(&mut self, doc: Vec<u8>) {
        host_lib::mongo_insert_one(&self.reservations_col, doc).await.unwrap()
    }
}

type SharedStore    = Arc<Mutex<Store<ReservationStoreData>>>;
type SharedInstance = Arc<ReservationStoreHostWorld>;

struct StoreGrpcService {
    store:    SharedStore,
    instance: SharedInstance,
}

#[tonic::async_trait]
impl ReservationStore for StoreGrpcService {
    async fn init(
        &self, _: Request<InitRequest>,
    ) -> Result<Response<InitResponse>, Status> {
        let mut s = self.store.lock().await;
        self.instance.hotel_reservation_data_reservation_store()
            .call_init(&mut *s).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_numbers(
        &self, _: Request<LoadNumbersRequest>,
    ) -> Result<Response<LoadNumbersResponse>, Status> {
        let mut s = self.store.lock().await;
        let nums = self.instance.hotel_reservation_data_reservation_store()
            .call_load_numbers(&mut *s).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadNumbersResponse {
            numbers: nums.into_iter().map(|n| ProtoNumberRec {
                hotel_id:       n.hotel_id,
                number_of_room: n.number_of_room,
            }).collect(),
        }))
    }

    async fn load_reservations(
        &self, _: Request<LoadReservationsRequest>,
    ) -> Result<Response<LoadReservationsResponse>, Status> {
        let mut s = self.store.lock().await;
        let recs = self.instance.hotel_reservation_data_reservation_store()
            .call_load_reservations(&mut *s).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadReservationsResponse {
            reservations: recs.into_iter().map(|r| ProtoReservationRec {
                hotel_id:      r.hotel_id,
                customer_name: r.customer_name,
                in_date:       r.in_date,
                out_date:      r.out_date,
                number:        r.number,
            }).collect(),
        }))
    }

    async fn insert_reservation(
        &self, req: Request<InsertReservationRequest>,
    ) -> Result<Response<InsertReservationResponse>, Status> {
        let r = req.into_inner();
        let mut s = self.store.lock().await;
        self.instance.hotel_reservation_data_reservation_store()
            .call_insert_reservation(&mut *s,
                hotel::reservation_data::reservation_store::ReservationRec {
                    hotel_id:      r.hotel_id,
                    customer_name: r.customer_name,
                    in_date:       r.in_date,
                    out_date:      r.out_date,
                    number:        r.number,
                },
            ).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InsertReservationResponse {}))
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mongo_uri   = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8101".into());
    let data_dir    = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file   = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "reservation-store.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let db    = mongo.database("reservation-db");
    let numbers_col      = Arc::new(db.collection::<bson::Document>("number"));
    let reservations_col = Arc::new(db.collection::<bson::Document>("reservation"));

    let engine     = host_lib::make_engine()?;
    let mut linker: Linker<ReservationStoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReservationStoreHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = ReservationStoreData {
        wasi:             host_lib::make_store_wasi_ctx(&data_dir)?,
        table:            wasmtime_wasi::ResourceTable::new(),
        numbers_col,
        reservations_col,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance  = ReservationStoreHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_reservation_data_reservation_store()
        .call_init(&mut store).await?;

    let store    = Arc::new(Mutex::new(store));
    let instance = Arc::new(instance);

    println!("reservation-host [store] listening on {listen_addr}");

    Server::builder()
        .add_service(ReservationStoreServer::new(StoreGrpcService { store, instance }))
        .serve(listen_addr.parse()?)
        .await?;
    Ok(())
}
```

### 15. `hosts/reservation/src/svc.rs`

The accessor for `hotel:reservation-data/reservation-store` → `hotel_reservation_data_reservation_store()`.
The accessor for `hotel:reservation/reservation` export → `hotel_reservation_reservation()`.

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::Linker;
use crate::grpc::ReservationComponent;

mod store_proto {
    tonic::include_proto!("reservation_store");
}
use store_proto::{
    reservation_store_client::ReservationStoreClient,
    InitRequest, LoadNumbersRequest, LoadReservationsRequest, InsertReservationRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/reservation/wit",
    world: "reservation-host-world",
    async: true,
});

pub struct HostData {
    pub wasi:         wasmtime_wasi::WasiCtx,
    pub table:        wasmtime_wasi::ResourceTable,
    pub store_client: Arc<Mutex<ReservationStoreClient<tonic::transport::Channel>>>,
    pub cache:        Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::reservation_data::reservation_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC reservation-store Init failed");
    }

    async fn load_numbers(
        &mut self,
    ) -> Vec<hotel::reservation_data::reservation_store::NumberRec> {
        let resp = self.store_client.lock().await
            .load_numbers(tonic::Request::new(LoadNumbersRequest {})).await
            .expect("gRPC reservation-store LoadNumbers failed")
            .into_inner();
        resp.numbers.into_iter().map(|n| {
            hotel::reservation_data::reservation_store::NumberRec {
                hotel_id:       n.hotel_id,
                number_of_room: n.number_of_room,
            }
        }).collect()
    }

    async fn load_reservations(
        &mut self,
    ) -> Vec<hotel::reservation_data::reservation_store::ReservationRec> {
        let resp = self.store_client.lock().await
            .load_reservations(tonic::Request::new(LoadReservationsRequest {})).await
            .expect("gRPC reservation-store LoadReservations failed")
            .into_inner();
        resp.reservations.into_iter().map(|r| {
            hotel::reservation_data::reservation_store::ReservationRec {
                hotel_id:      r.hotel_id,
                customer_name: r.customer_name,
                in_date:       r.in_date,
                out_date:      r.out_date,
                number:        r.number,
            }
        }).collect()
    }

    async fn insert_reservation(
        &mut self,
        r: hotel::reservation_data::reservation_store::ReservationRec,
    ) {
        self.store_client.lock().await
            .insert_reservation(tonic::Request::new(InsertReservationRequest {
                hotel_id:      r.hotel_id,
                customer_name: r.customer_name,
                in_date:       r.in_date,
                out_date:      r.out_date,
                number:        r.number,
            })).await
            .expect("gRPC reservation-store InsertReservation failed");
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
impl ReservationComponent for ReservationHostWorld {
    type Data = HostData;

    async fn check_availability(
        &self,
        store:       &mut Store<HostData>,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>> {
        let result = self.hotel_reservation_reservation()
            .call_check_availability(store, &hotel_ids, &in_date, &out_date, room_number)
            .await?;
        Ok(result)
    }

    async fn make_reservation(
        &self,
        store:         &mut Store<HostData>,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>> {
        let result = self.hotel_reservation_reservation()
            .call_make_reservation(store, &hotel_id, &customer_name, &in_date, &out_date, room_number)
            .await?;
        Ok(result)
    }
}

pub async fn run() -> anyhow::Result<()> {
    use wasmtime::component::Component;

    let store_addr  = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8101".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8100".into())
        .parse()?;
    let wasm_file   = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "reservation.wasm".into());

    let store_client = ReservationStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    let engine     = host_lib::make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReservationHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi:         host_lib::make_wasi_ctx(),
        table:        wasmtime_wasi::ResourceTable::new(),
        store_client,
        cache,
    };
    let mut store = wasmtime::Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance  = ReservationHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_reservation_reservation().call_init(&mut store).await?;

    println!("reservation-host [svc] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
```

### 16. `hosts/reservation/src/abi.rs`

`AbiData` needs two collection fields (same as `ReservationStoreData` in store.rs, plus cache).
The accessor for the composed world export is `hotel_reservation_reservation()`.

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::{Component, Linker};
use crate::grpc::ReservationComponent;

wasmtime::component::bindgen!({
    path: "../../components/reservation/wit",
    world: "reservation-composed-host-world",
    async: true,
});

pub struct AbiData {
    pub wasi:             wasmtime_wasi::WasiCtx,
    pub table:            wasmtime_wasi::ResourceTable,
    pub numbers_col:      Arc<mongodb::Collection<bson::Document>>,
    pub reservations_col: Arc<mongodb::Collection<bson::Document>>,
    pub cache:            Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl wasmtime_wasi::WasiView for AbiData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::reservation_data::numbers_col::Host for AbiData {
    async fn count(&mut self) -> u64 {
        host_lib::mongo_count(&self.numbers_col).await.unwrap_or(0)
    }
    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        host_lib::mongo_find_all(&self.numbers_col).await.unwrap_or_default()
    }
    async fn insert_many(&mut self, docs: Vec<Vec<u8>>) {
        host_lib::mongo_insert_many(&self.numbers_col, docs).await.unwrap()
    }
}

#[async_trait::async_trait]
impl hotel::reservation_data::reservations_col::Host for AbiData {
    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        host_lib::mongo_find_all(&self.reservations_col).await.unwrap_or_default()
    }
    async fn insert_one(&mut self, doc: Vec<u8>) {
        host_lib::mongo_insert_one(&self.reservations_col, doc).await.unwrap()
    }
}

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
impl ReservationComponent for ReservationComposedHostWorld {
    type Data = AbiData;

    async fn check_availability(
        &self,
        store:       &mut Store<AbiData>,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_reservation_reservation()
            .call_check_availability(store, &hotel_ids, &in_date, &out_date, room_number)
            .await?)
    }

    async fn make_reservation(
        &self,
        store:         &mut Store<AbiData>,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_reservation_reservation()
            .call_make_reservation(store, &hotel_id, &customer_name, &in_date, &out_date, room_number)
            .await?)
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mongo_uri   = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8100".into())
        .parse()?;
    let data_dir    = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file   = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "reservation-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let db    = mongo.database("reservation-db");
    let numbers_col      = Arc::new(db.collection::<bson::Document>("number"));
    let reservations_col = Arc::new(db.collection::<bson::Document>("reservation"));
    let cache: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    let engine     = host_lib::make_engine()?;
    let mut linker: Linker<AbiData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReservationComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = AbiData {
        wasi:             host_lib::make_store_wasi_ctx(&data_dir)?,
        table:            wasmtime_wasi::ResourceTable::new(),
        numbers_col,
        reservations_col,
        cache,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance  = ReservationComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_reservation_reservation().call_init(&mut store).await?;

    println!("reservation-host [abi] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
```

---

## Files to modify

### `host-lib/src/lib.rs`

Add after `mongo_insert_many`:

```rust
pub async fn mongo_insert_one(
    collection: &Arc<mongodb::Collection<bson::Document>>,
    doc: Vec<u8>,
) -> Result<()> {
    let bson_doc = json_to_bson(&doc)?;
    collection.insert_one(bson_doc).await?;
    Ok(())
}
```

### `Cargo.toml` (workspace root)

Add `"hosts/reservation"` to the `members` array.

### `Makefile`

Add `reservation` to the `SERVICES` variable:

```makefile
SERVICES := recommendation attractions geo user rate profile review reservation
```

### `docker-compose.tcp.yml`

Add before `volumes:` (after review services):

```yaml
  mongodb-reservation:
    image: mongo:5.0
    volumes:
      - reservation-data:/data/db

  reservation-store:
    build:
      context: .
      target: runtime
      args:
        SERVICE: reservation
    environment:
      MODE: store
      MONGO_URI: "mongodb://mongodb-reservation:27017"
      LISTEN_ADDR: "0.0.0.0:8101"
      DATA_DIR: /data
      WASM_FILE: /app/reservation-store.wasm
    ports:
      - "8101:8101"
    volumes:
      - ./data/reservation-numbers-seed.json:/data/reservation-numbers-seed.json:ro
      - ./data/reservation-reservations-seed.json:/data/reservation-reservations-seed.json:ro
    depends_on:
      - mongodb-reservation
    restart: on-failure

  reservation:
    build:
      context: .
      target: runtime
      args:
        SERVICE: reservation
    environment:
      MODE: svc
      STORE_ADDR: "http://reservation-store:8101"
      LISTEN_ADDR: "0.0.0.0:8100"
      WASM_FILE: /app/reservation.wasm
    ports:
      - "8100:8100"
    depends_on:
      - reservation-store
    restart: on-failure
```

Also add `reservation-data:` under `volumes:`.

### `docker-compose.abi.yml`

Add before `volumes:`:

```yaml
  mongodb-reservation:
    image: mongo:5.0
    volumes:
      - reservation-data:/data/db

  reservation:
    build:
      context: .
      target: runtime
      args:
        SERVICE: reservation
    environment:
      MODE: abi
      MONGO_URI: "mongodb://mongodb-reservation:27017"
      LISTEN_ADDR: "0.0.0.0:8100"
      DATA_DIR: /data
      WASM_FILE: /app/reservation-composed.wasm
    ports:
      - "8100:8100"
    volumes:
      - ./data/reservation-numbers-seed.json:/data/reservation-numbers-seed.json:ro
      - ./data/reservation-reservations-seed.json:/data/reservation-reservations-seed.json:ro
    depends_on:
      - mongodb-reservation
    restart: on-failure
```

Also add `reservation-data:` under `volumes:`.

---

## Common pitfalls specific to this service

- **`define_store_service!` and `run_store!` macros do NOT apply.** Both assume
  `host_lib::StoreData` with one `collection` field. Write the store's `StoreGrpcService`,
  type aliases, and `run()` function manually (as shown above).

- **`impl_collection_host!` macro does NOT apply.** Implement `numbers_col::Host` and
  `reservations_col::Host` manually on `ReservationStoreData` and `AbiData`.

- **`mongo_insert_one` must be added to host-lib first.** The reservation store and abi hosts
  call it. If you forget, you'll get a missing function error.

- **WIT local self-imports**: The `reservation_store` WIT package imports
  `hotel:reservation-data/numbers-col` and `hotel:reservation-data/reservations-col` — both
  defined in the same file. `wkg wit fetch` treats them as local and does not try to resolve
  them from any registry. No `wkg.toml` override is needed for them.

- **`wkg.toml` for `reservation_store` must still exist** (even if empty of overrides) for
  `wkg wit fetch` to run for WASI dependency resolution. Create the file with just `[overrides]`.

- **Two seed files**, two collections, two mount paths in docker-compose. Don't forget to mount
  both seed files in the store container (and abi container).

- **Date pair logic**: `inDate="2015-04-09"`, `outDate="2015-04-10"` is a single overnight stay
  → one slot `("2015-04-09", "2015-04-10")`. `outDate="2015-04-11"` → two slots. The seed
  reservation already uses this one-slot-per-day representation.

- **`make-reservation` takes a single `hotel-id` string**, not a list. In `grpc.rs`,
  `MakeReservation` extracts `r.hotel_id.into_iter().next().unwrap_or_default()` from the proto
  `repeated string hotelId` field.

- **Rust field names**: `number-of-room` → `number_of_room`, `customer-name` → `customer_name`,
  `in-date` → `in_date`, `out-date` → `out_date`. Run `cargo check` to confirm.

- **`ReservationRec` in the store Rust bindgen** lives at
  `hotel::reservation_data::reservation_store::ReservationRec`. When you pass it to
  `call_insert_reservation`, construct it by that full path.

---

## How to test once built

```bash
# Build
make build/reservation-store.wasm build/reservation.wasm build/reservation-composed.wasm
cargo build -p reservation-host

# Run store (requires MongoDB on :27017)
DATA_DIR=data WASM_FILE=build/reservation-store.wasm MODE=store ./target/debug/reservation-host &

# Run svc
WASM_FILE=build/reservation.wasm MODE=svc STORE_ADDR=http://localhost:8101 \
  ./target/debug/reservation-host &

# Check availability
grpcurl -plaintext \
  -import-path ./proto -proto reservation.proto \
  -d '{"hotelId":["1","2","3"],"inDate":"2015-04-09","outDate":"2015-04-10","roomNumber":1}' \
  localhost:8100 reservation.Reservation/CheckAvailability

# Make a reservation
grpcurl -plaintext \
  -import-path ./proto -proto reservation.proto \
  -d '{"customerName":"Bob","hotelId":["1"],"inDate":"2015-04-09","outDate":"2015-04-10","roomNumber":1}' \
  localhost:8100 reservation.Reservation/MakeReservation
```
