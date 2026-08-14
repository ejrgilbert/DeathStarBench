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
	loadAll  func() []RatePlan,
	cacheGetMulti func([]string) [][]byte,
	cacheSet func(string, []byte),
) []RatePlan {
	var result []RatePlan
	var missed []string

	// One batched cache probe for all hotels (matches native GetMulti).
	vals := cacheGetMulti(hotelIds)
	for i, id := range hotelIds {
		if vals[i] != nil {
			var plans []RatePlan
			if err := json.Unmarshal(vals[i], &plans); err == nil {
				result = append(result, plans...)
				continue
			}
		}
		missed = append(missed, id)
	}

	if len(missed) > 0 {
		byHotel := make(map[string][]RatePlan)
		for _, p := range loadAll() {
			byHotel[p.HotelId] = append(byHotel[p.HotelId], p)
		}

		for hotelId, plans := range byHotel {
			if val, err := json.Marshal(plans); err != nil {
				println("rate: failed to marshal plans for hotel", hotelId, ":", err.Error())
			} else {
				cacheSet(hotelId, val)
			}
		}
		for _, id := range missed {
			result = append(result, byHotel[id]...)
		}
	}

	if len(hotelIds) > 0 {
		idSet := make(map[string]struct{}, len(hotelIds))
		for _, id := range hotelIds {
			idSet[id] = struct{}{}
		}
		filtered := result[:0:0]
		for _, p := range result {
			if _, ok := idSet[p.HotelId]; ok {
				filtered = append(filtered, p)
			}
		}
		result = filtered
	}

	sort.Slice(result, func(i, j int) bool {
		return result[i].RoomType.TotalRate > result[j].RoomType.TotalRate
	})
	return result
}
