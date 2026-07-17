package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	col "hotel-components/components/geo_store/host/storage/collection"
	attstore "hotel-components/components/geo_store/hotel/store/geo-store"
)

type seedPoint struct {
	ID  string  `json:"id"`
	Lat float64 `json:"lat"`
	Lon float64 `json:"lon"`
}

var (
	conn      col.Connection
	points    []attstore.Point
	allLoaded bool
)

func main() {}

func init() {
	attstore.Exports.Init = doInit
	attstore.Exports.LoadGeo = loadGeo
}

func doInit() {
	conn = col.ConnectionOpen("geo")
	if col.Count(conn) > 0 {
		return
	}

	data, err := os.ReadFile("/data/geo-seed.json")
	if err != nil {
		panic("read seed file: " + err.Error())
	}

	var seeds []seedPoint
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
		var s seedPoint
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		points = append(points, attstore.Point{ID: s.ID, Lat: s.Lat, Lon: s.Lon})
	}
	allLoaded = true
}

func loadGeo() cm.List[attstore.Point] {
	ensureLoaded()
	return cm.ToList(points)
}
