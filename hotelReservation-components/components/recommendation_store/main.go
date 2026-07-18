package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	col      "hotel-components/components/recommendation_store/host/storage/collection"
	recstore "hotel-components/components/recommendation_store/hotel/store/recommendation-store"
)

type seedRecord struct {
	ID    string  `json:"id"`
	Lat   float64 `json:"lat"`
	Lon   float64 `json:"lon"`
	Rate  float64 `json:"rate"`
	Price float64 `json:"price"`
}

var (
	conn     col.Connection
	connOpen bool
)

func main() {}

func init() {
	recstore.Exports.LoadHotels = loadHotels
}

func ensureConn() {
	if connOpen {
		return
	}
	conn = col.ConnectionOpen("recs")
	connOpen = true
}

func loadHotels() cm.List[recstore.Hotel] {
	ensureConn()
	rawDocs := col.FindAll(conn).Slice()
	hotels := make([]recstore.Hotel, len(rawDocs))
	for i, raw := range rawDocs {
		var s seedRecord
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		hotels[i] = recstore.Hotel{ID: s.ID, Lat: s.Lat, Lon: s.Lon, Rate: s.Rate, Price: s.Price}
	}
	return cm.ToList(hotels)
}
