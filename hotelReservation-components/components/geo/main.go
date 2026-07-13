package main

import (
	"go.bytecodealliance.org/cm"

	store "hotel-components/components/geo/hotel/geo-data/geo-store"
	attapi "hotel-components/components/geo/hotel/geo/geo"
)

var svc = NewService()

func main() {}

func init() {
	attapi.Exports.Init = doInit
	attapi.Exports.Nearby = nearby
}

func doInit() {
	store.Init()

	witGeo := store.LoadGeo().Slice()
	geo := make([]Point, len(witGeo))
	for i, r := range witGeo {
		geo[i] = Point{id: r.ID, plat: r.Lat, plon: r.Lon}
	}

	svc.Load(geo)
}

func nearby(lat float64, lon float64) cm.List[string] {
	ids, _ := svc.Nearby(lat, lon)
	return cm.ToList(ids)
}
