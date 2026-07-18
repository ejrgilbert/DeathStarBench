package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	col      "hotel-components/components/geo_store/host/storage/collection"
	geostore "hotel-components/components/geo_store/hotel/store/geo-store"
)

type seedPoint struct {
	ID  string  `json:"id"`
	Lat float64 `json:"lat"`
	Lon float64 `json:"lon"`
}

var (
	conn      col.Connection
	connOpen  bool
	points    []geostore.Point
	allLoaded bool
)

func main() {}

func init() {
	geostore.Exports.LoadGeo = loadGeo
}

func ensureConn() {
	if connOpen {
		return
	}
	conn = col.ConnectionOpen("geo")
	connOpen = true
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	ensureConn()
	rawDocs := col.FindAll(conn).Slice()
	for _, raw := range rawDocs {
		var s seedPoint
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		points = append(points, geostore.Point{ID: s.ID, Lat: s.Lat, Lon: s.Lon})
	}
	allLoaded = true
}

func loadGeo() cm.List[geostore.Point] {
	ensureLoaded()
	return cm.ToList(points)
}
