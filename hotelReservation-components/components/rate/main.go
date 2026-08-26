package main

import (

	"go.bytecodealliance.org/cm"

	hkv      "hotel-components/components/rate/cache/keyvalue/keyvalue"
	store    "hotel-components/components/rate/hotel/store/rate-store"
	rateapi  "hotel-components/components/rate/hotel/api/rate"
)

var svc = NewService()

func main() {}

func init() {
	rateapi.Exports.GetRates = getRates
}

func getRates(hotelIds cm.List[string], inDate, outDate string) (result cm.List[rateapi.RatePlan]) {
	lowerIds := make([]string, len(hotelIds.Slice()))
	for i, id := range hotelIds.Slice() {
	    lowerIds[i] = id
	}
	plans := svc.GetRates(lowerIds, loadAll, cacheGetMulti, cacheSet)

	witResult := make([]rateapi.RatePlan, len(plans))
	for i, p := range plans {
		witResult[i] = rateapi.RatePlan{
			HotelID: p.HotelId,
			Code:    p.Code,
			InDate:  p.InDate,
			OutDate: p.OutDate,
			RoomType: rateapi.RoomType{
				BookableRate:       p.RoomType.BookableRate,
				Code:               p.RoomType.Code,
				RoomDescription:    p.RoomType.RoomDescription,
				TotalRate:          p.RoomType.TotalRate,
				TotalRateInclusive: p.RoomType.TotalRateInclusive,
			},
		}
	}
	result = cm.ToList(witResult)
	return
}

func loadAll() []RatePlan {
	witPlans := store.LoadRates().Slice()
	plans := make([]RatePlan, len(witPlans))
	for i, wp := range witPlans {
		plans[i] = RatePlan{
			HotelId: wp.HotelID,
			Code:    wp.Code,
			InDate:  wp.InDate,
			OutDate: wp.OutDate,
			RoomType: RoomType{
				BookableRate:       wp.RoomType.BookableRate,
				Code:               wp.RoomType.Code,
				RoomDescription:    wp.RoomType.RoomDescription,
				TotalRate:          wp.RoomType.TotalRate,
				TotalRateInclusive: wp.RoomType.TotalRateInclusive,
			},
		}
	}
	return plans
}

const cacheNS = "rate:"

// cacheGetMulti probes all keys in one batched call (native memcached GetMulti).
// The returned slice is parallel to keys; a nil entry is a cache miss.
func cacheGetMulti(keys []string) [][]byte {
	nsKeys := make([]string, len(keys))
	for i, k := range keys {
		nsKeys[i] = cacheNS + k
	}
	res := hkv.GetMulti(cm.ToList(nsKeys)).Slice()
	out := make([][]byte, len(keys))
	for i := range keys {
		if i < len(res) && !res[i].None() {
			out[i] = res[i].Some().Slice()
		}
	}
	return out
}

func cacheSet(key string, val []byte) {
	hkv.Set(cacheNS+key, cm.ToList(val))
}
