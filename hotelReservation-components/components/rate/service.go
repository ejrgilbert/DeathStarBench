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
	// QUIRK REPLICATED FROM native services/rate/server.go GetRates (kept identical
	// for a fair native-vs-component comparison; both the empty-filter read and the
	// per-hotel duplication below are the original's behavior, not the port's):
	//
	//  (1) EMPTY-filter read: the original does collection.Find(bson.D{}), returning
	//      EVERY rate plan regardless of the geo-computed hotelIds or the requested
	//      in/out dates (geo-proximity filtering is a no-op in the original).
	//  (2) PER-HOTEL DUPLICATION: the original loops over the requested hotelIds and,
	//      for each one (whether the per-hotel cache Get hits or misses), appends the
	//      ENTIRE rate set — so N requested hotels yield N*|rates| plans, and the
	//      full-set blob is cached under EACH hotelId. With geo returning
	//      maxSearchResults hotels this inflates search's candidate list ~N-fold,
	//      which is what drives the downstream CheckAvailability cache/DB load. We
	//      reproduce it so that load matches the original exactly (a plain dedup here
	//      would make the port do far less reservation-cache work than native).
	if len(hotelIds) == 0 {
		return nil
	}

	cached := cacheGetMulti(hotelIds) // one batched probe, parallel to hotelIds
	var full []RatePlan               // the whole rate set, loaded once on the first miss
	var result []RatePlan
	for i, id := range hotelIds {
		if cached[i] != nil {
			var plans []RatePlan
			if err := json.Unmarshal(cached[i], &plans); err == nil && len(plans) > 0 {
				result = append(result, plans...) // cache hit: the cached blob is the full set
				continue
			}
		}
		// miss: (re)load the whole set (matching the original's per-missed-hotel
		// Find(bson.D{})), cache it under this hotelId, and append it.
		if full == nil {
			full = loadAll()
		}
		if blob, err := json.Marshal(full); err != nil {
			println("rate: failed to marshal plans:", err.Error())
		} else {
			cacheSet(id, blob)
		}
		result = append(result, full...)
	}

	sort.Slice(result, func(i, j int) bool {
		return result[i].RoomType.TotalRate > result[j].RoomType.TotalRate
	})
	return result
}
