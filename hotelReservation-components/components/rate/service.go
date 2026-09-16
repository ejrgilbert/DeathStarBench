package main

import (
	"encoding/json"
	"sort"
)

type RoomType struct {
	BookableRate       float64
	Code               string
	RoomDescription    string
	TotalRate          float64
	TotalRateInclusive float64
}

type RatePlan struct {
	HotelId  string
	Code     string
	InDate   string
	OutDate  string
	RoomType RoomType
}

type Service struct{}

func NewService() *Service { return &Service{} }

func (s *Service) GetRates(
	hotelIds []string,
	loadAll func() []RatePlan,
	cacheGetMulti func([]string) [][]byte,
	cacheSet func(string, []byte),
) []RatePlan {
	// REPRO of BUG in original(equivalence):
	// the original rate service (services/rate/server.go) reads mongo with an EMPTY filter:
	//     collection.Find(bson.D{})
	// so it returns EVERY rate plan regardless of the hotelIds that geo computed, and
	// regardless of the requested in/out dates. Making geo-proximity filtering noop.
	if len(hotelIds) == 0 {
		return nil
	}

	var all []RatePlan
	for _, v := range cacheGetMulti(hotelIds) {
		if v == nil {
			continue
		}
		var plans []RatePlan
		if err := json.Unmarshal(v, &plans); err == nil && len(plans) > 0 {
			all = plans
			break
		}
	}
	if len(all) == 0 {
		all = loadAll()
		if blob, err := json.Marshal(all); err != nil {
			println("rate: failed to marshal plans:", err.Error())
		} else {
			for _, id := range hotelIds {
				cacheSet(id, blob)
			}
		}
	}

	sort.Slice(all, func(i, j int) bool {
		return all[i].RoomType.TotalRate > all[j].RoomType.TotalRate
	})
	return all
}
