package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	hkv   "hotel-components/components/recommendation/host/cache/keyvalue"
	store "hotel-components/components/recommendation/hotel/store/recommendation-store"
	recapi "hotel-components/components/recommendation/hotel/api/recommendation"
)

func main() {}

func init() {
	recapi.Exports.Recommend = recommend
}

func loadHotels() []Hotel {
	const key = "rec:hotels"
	if opt := hkv.Get(key); !opt.None() {
		var hotels []Hotel
		if err := json.Unmarshal(opt.Some().Slice(), &hotels); err == nil {
			return hotels
		}
	}
	witHotels := store.LoadHotels().Slice()
	hotels := make([]Hotel, len(witHotels))
	for i, h := range witHotels {
		hotels[i] = Hotel{
			ID:    string([]byte(h.ID)),
			Lat:   h.Lat,
			Lon:   h.Lon,
			Rate:  h.Rate,
			Price: h.Price,
		}
	}
	if b, err := json.Marshal(hotels); err == nil {
		hkv.Set(key, cm.ToList(b))
	}
	return hotels
}

func recommend(req recapi.Requirement, lat float64, lon float64) cm.List[string] {
	svc := NewService()
	svc.Load(loadHotels())

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
