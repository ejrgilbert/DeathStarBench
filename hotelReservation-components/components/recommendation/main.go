package main

import (
	"go.bytecodealliance.org/cm"


	store "hotel-components/components/recommendation/hotel/store/recommendation-store"
	recapi "hotel-components/components/recommendation/hotel/api/recommendation"
)

func main() {}

func init() {
	recapi.Exports.Recommend = recommend
}

// svc holds the hotels dataset, loaded once on the first request and reused
// across requests — matching the Go original (s.hotels = loadRecommendations(...)
// once in Server.Run(), then per-request scoring against the in-memory set).
var svc *Service

func loadHotels() []Hotel {
	witHotels := store.LoadHotels().Slice()
	hotels := make([]Hotel, len(witHotels))
	for i, h := range witHotels {
		hotels[i] = Hotel{
			ID:    h.ID,
			Lat:   h.Lat,
			Lon:   h.Lon,
			Rate:  h.Rate,
			Price: h.Price,
		}
	}
	return hotels
}

func recommend(req recapi.Requirement, lat float64, lon float64) cm.List[string] {
	if svc == nil {
		svc = NewService()
		svc.Load(loadHotels())
	}

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
