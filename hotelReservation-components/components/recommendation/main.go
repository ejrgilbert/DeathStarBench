package main

import (
	"go.bytecodealliance.org/cm"

	store "hotel-components/components/recommendation/hotel/store/recommendation-store"
	recapi "hotel-components/components/recommendation/hotel/api/recommendation"
)

var svc = NewService()

func main() {}

func init() {
	recapi.Exports.Init = doInit
	recapi.Exports.Recommend = recommend
}

func doInit() {
	store.Init()

	witHotels := store.LoadHotels().Slice()
	hotels := make([]Hotel, len(witHotels))
	for i, h := range witHotels {
		hotels[i] = Hotel{ID: h.ID, Lat: h.Lat, Lon: h.Lon, Rate: h.Rate, Price: h.Price}
	}
	svc.Load(hotels)
}

func recommend(req recapi.Requirement, lat float64, lon float64) cm.List[string] {
	var r Requirement
	switch req {
	case recapi.RequirementDistance:
		r = RequirementDistance
	case recapi.RequirementRate:
		r = RequirementRate
	case recapi.RequirementPrice:
		r = RequirementPrice
	}

	ids, _ := svc.Recommend(r, lat, lon)
	return cm.ToList(ids)
}
