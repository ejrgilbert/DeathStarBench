package main

import (
	"go.bytecodealliance.org/cm"


	store  "hotel-components/components/geo/hotel/store/geo-store"
	attapi "hotel-components/components/geo/hotel/api/geo"
)

func main() {}

func init() {
	attapi.Exports.Nearby = nearby
}

// svc holds the geo index, built once on the first request and reused across
// requests — matching the Go original (s.index = newGeoIndex(...) once in
// Server.Run(), then per-request nearest-neighbour queries against it).
var svc *Service

func loadGeo() []Point {
	witGeo := store.LoadGeo().Slice()
	points := make([]Point, len(witGeo))
	for i, r := range witGeo {
		points[i] = Point{id: r.ID, plat: r.Lat, plon: r.Lon}
	}
	return points
}

func nearby(lat float64, lon float64) cm.List[string] {
	if svc == nil {
		svc = NewService()
		svc.Load(loadGeo())
	}
	ids, _ := svc.Nearby(lat, lon)
	return cm.ToList(ids)
}
