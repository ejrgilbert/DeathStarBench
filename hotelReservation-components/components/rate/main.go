package main

import (
	gcutil "hotel-components/internal/gcutil"

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
    // tinygo string bug workaround
	lowerIds := make([]string, len(hotelIds.Slice()))
	for i, id := range hotelIds.Slice() {
	    lowerIds[i] = string([]byte(id))
	}
	plans := svc.GetRates(lowerIds, loadAll, cacheGetMulti, cacheSet)

	witResult := make([]rateapi.RatePlan, len(plans))
	for i, p := range plans {
		witResult[i] = rateapi.RatePlan{
			HotelID: string([]byte(p.HotelId)),
			Code:    string([]byte(p.Code)),
			InDate:  string([]byte(p.InDate)),
			OutDate: string([]byte(p.OutDate)),
			RoomType: rateapi.RoomType{
				BookableRate:       p.RoomType.BookableRate,
				Code:               string([]byte(p.RoomType.Code)),
				RoomDescription:    string([]byte(p.RoomType.RoomDescription)),
				TotalRate:          p.RoomType.TotalRate,
				TotalRateInclusive: p.RoomType.TotalRateInclusive,
			},
		}
	}
	gcutil.Tick()
	result = cm.ToList(witResult)
	return
}

func loadAll() []RatePlan {
	witPlans := store.LoadRates().Slice()
	plans := make([]RatePlan, len(witPlans))
	for i, wp := range witPlans {
		plans[i] = RatePlan{
			HotelId: string([]byte(wp.HotelID)),
			Code:    string([]byte(wp.Code)),
			InDate:  string([]byte(wp.InDate)),
			OutDate: string([]byte(wp.OutDate)),
			RoomType: RoomType{
				BookableRate:       wp.RoomType.BookableRate,
				Code:               string([]byte(wp.RoomType.Code)),
				RoomDescription:    string([]byte(wp.RoomType.RoomDescription)),
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
			// tinyGo bug workaround
			src := res[i].Some().Slice()
			dst := make([]byte, len(src))
			copy(dst, src)
			out[i] = dst
		}
	}
	return out
}

func cacheSet(key string, val []byte) {
	hkv.Set(cacheNS+key, cm.ToList(val))
}
