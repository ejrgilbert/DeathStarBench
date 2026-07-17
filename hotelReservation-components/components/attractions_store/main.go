package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	col "hotel-components/components/attractions_store/host/storage/collection"
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
	hotelPositions []attstore.HotelPosition
	restaurants    []attstore.Restaurant
	museums        []attstore.Museum
	cinemas        []attstore.Cinema
	allLoaded      bool
)

func main() {}

func init() {
	attstore.Exports.Init = doInit
	attstore.Exports.LoadHotelPositions = loadHotelPositions
	attstore.Exports.LoadRestaurants = loadRestaurants
	attstore.Exports.LoadMuseums = loadMuseums
	attstore.Exports.LoadCinemas = loadCinemas
}

func doInit() {
	conn = col.ConnectionOpen("attractions")
	if col.Count(conn) > 0 {
		return
	}

	data, err := os.ReadFile("/data/attractions-seed.json")
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
	col.InsertMany(conn, cm.ToList(docs))
}

func ensureLoaded() {
	if allLoaded {
		return
	}
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

func loadHotelPositions() cm.List[attstore.HotelPosition] { ensureLoaded(); return cm.ToList(hotelPositions) }
func loadRestaurants() cm.List[attstore.Restaurant]        { ensureLoaded(); return cm.ToList(restaurants) }
func loadMuseums() cm.List[attstore.Museum]                { ensureLoaded(); return cm.ToList(museums) }
func loadCinemas() cm.List[attstore.Cinema]                { ensureLoaded(); return cm.ToList(cinemas) }
