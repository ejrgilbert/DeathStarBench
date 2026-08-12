package main

import (
	gcutil "hotel-components/internal/gcutil"

	"go.bytecodealliance.org/cm"

	hkv      "hotel-components/components/rate/host/cache/keyvalue"
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
	plans := svc.GetRates(lowerIds, loadAll, cacheGet, cacheSet)

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
	gcutil.Tick()
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

func cacheGet(key string) ([]byte, bool) {
	opt := hkv.Get(key)
	if opt.None() {
		return nil, false
	}
	return opt.Some().Slice(), true
}

func cacheSet(key string, val []byte) {
	hkv.Set(key, cm.ToList(val))
}
