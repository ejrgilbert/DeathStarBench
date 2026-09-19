package main

import (
	"encoding/json"
	"fmt"

	"go.bytecodealliance.org/cm"

	col      "hotel-components/components/attractions_store/host/storage/collection"
	attstore "hotel-components/components/attractions_store/hotel/store/attractions-store"
)

type seedRecord struct {
	Type     string  `json:"type"`
	ID       string  `json:"id"`
	Lat      float64 `json:"lat"`
	Lon      float64 `json:"lon"`
	Name     string  `json:"name,omitempty"`
	Rating   float32 `json:"rating,omitempty"`
	Category string  `json:"category,omitempty"`
}

var (
	conn           col.Connection
	connOpen       bool
	hotelPositions []attstore.HotelPosition
	restaurants    []attstore.Restaurant
	museums        []attstore.Museum
	cinemas        []attstore.Cinema
	allLoaded      bool
)

func main() {}

func init() {
	attstore.Exports.LoadHotelPositions = loadHotelPositions
	attstore.Exports.LoadRestaurants    = loadRestaurants
	attstore.Exports.LoadMuseums        = loadMuseums
	attstore.Exports.LoadCinemas        = loadCinemas
	attstore.Exports.GetHotelPosition   = getHotelPosition
}

func ensureConn() {
	if connOpen {
		return
	}
	conn = col.ConnectionOpen("attractions")
	connOpen = true
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	ensureConn()
	rawDocs := col.FindAll(conn).Slice()
	for _, raw := range rawDocs {
		var s seedRecord
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		switch s.Type {
		case "hotel":
			hotelPositions = append(hotelPositions, attstore.HotelPosition{ID: s.ID, Lat: s.Lat, Lon: s.Lon})
		case "restaurant":
			restaurants = append(restaurants, attstore.Restaurant{ID: s.ID, Lat: s.Lat, Lon: s.Lon, Name: s.Name, Rating: s.Rating, Category: s.Category})
		case "museum":
			museums = append(museums, attstore.Museum{ID: s.ID, Lat: s.Lat, Lon: s.Lon, Name: s.Name, Category: s.Category})
		case "cinema":
			cinemas = append(cinemas, attstore.Cinema{ID: s.ID, Lat: s.Lat, Lon: s.Lon, Name: s.Name, Category: s.Category})
		}
	}
	allLoaded = true
}

// getHotelPosition resolves the query hotel's coordinates with a targeted
// per-request Mongo query, replicating a native attractions-service quirk (see
// services/attractions/server.go NearbyRest/Mus/Cinema): the native service
// re-queries the hotel's lat/lon on EVERY request rather than caching it, even
// though it already loaded every hotel into an in-memory geo index at boot.
//
// It is deliberately wonky to match native EXACTLY:
//   - native issues `Find({"hotelId": id})` (a multi-doc cursor), not a FindOne,
//     so we use col.Find here too — both COLLSCAN and bump the Mongo `query`
//     opcounter by 1/request (the fairness-relevant count), giving ~60/kind.
//   - native ranges over the cursor keeping the LAST match; hotel ids are unique
//     so there is only ever one, but we mirror the take-last loop so the behavior
//     is identical if the collection ever held dup ids.
//
// Seed records carry a `type` discriminator (the port folds native's separate
// hotels/restaurants/... collections into one `attractions` collection), so hotel
// rows are keyed by {type:"hotel", id} — the documented state-normalization fold,
// not a behavioral divergence.
func getHotelPosition(hotelID string) cm.Option[attstore.HotelPosition] {
	ensureConn()
	filter := fmt.Sprintf(`{"type":"hotel","id":%q}`, hotelID)
	docs := col.Find(conn, col.Document(cm.ToList([]uint8(filter)))).Slice()
	if len(docs) == 0 {
		return cm.None[attstore.HotelPosition]()
	}
	var s seedRecord
	for _, raw := range docs {
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
	}
	return cm.Some(attstore.HotelPosition{ID: s.ID, Lat: s.Lat, Lon: s.Lon})
}

func loadHotelPositions() cm.List[attstore.HotelPosition] { ensureLoaded(); return cm.ToList(hotelPositions) }
func loadRestaurants() cm.List[attstore.Restaurant]        { ensureLoaded(); return cm.ToList(restaurants) }
func loadMuseums() cm.List[attstore.Museum]                { ensureLoaded(); return cm.ToList(museums) }
func loadCinemas() cm.List[attstore.Cinema]                { ensureLoaded(); return cm.ToList(cinemas) }
