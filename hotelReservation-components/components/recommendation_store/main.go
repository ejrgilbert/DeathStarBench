package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	col "hotel-components/components/recommendation_store/host/storage/collection"
	recstore "hotel-components/components/recommendation_store/hotel/recommendation-data/recommendation-store"
)

type seedRecord struct {
	ID    string  `json:"id"`
	Lat   float64 `json:"lat"`
	Lon   float64 `json:"lon"`
	Rate  float64 `json:"rate"`
	Price float64 `json:"price"`
}

func main() {}

func init() {
	recstore.Exports.Init = init
	recstore.Exports.LoadHotels = loadHotels
}

func init() {
	if col.Count() > 0 {
		return
	}

	data, err := os.ReadFile("/data/recommendation-seed.json")
	if err != nil {
		panic("read seed file: " + err.Error())
	}

	var seeds []seedRecord
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

func loadHotels() cm.List[recstore.Hotel] {
	rawDocs := col.FindAll().Slice()
	hotels := make([]recstore.Hotel, len(rawDocs))
	for i, raw := range rawDocs {
		var s seedRecord
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		hotels[i] = recstore.Hotel{ID: s.ID, Lat: s.Lat, Lon: s.Lon, Rate: s.Rate, Price: s.Price}
	}
	return cm.ToList(hotels)
}
