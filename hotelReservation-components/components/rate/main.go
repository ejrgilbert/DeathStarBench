package main

import (
	"go.bytecodealliance.org/cm"

	kv "hotel-components/components/rate/cache/keyvalue/keyvalue"
	store "hotel-components/components/rate/hotel/store/rate-store"
	rateapi "hotel-components/components/rate/hotel/api/rate"
)

var svc = NewService()

func main() {}

func init() {
	rateapi.Exports.Init = doInit
	rateapi.Exports.GetRates = getRates
}

func doInit() {
	store.Init()
}

func getRates(hotelIds cm.List[string], inDate, outDate string) (result cm.List[rateapi.RatePlan]) {
	plans := svc.GetRates(hotelIds.Slice(), loadAll, cacheGet, cacheSet)

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
        // bug workaround: string([]byte(s)) copies data out of the WIT-allocated buffer into
        // Go-managed heap memory, preventing the GC from collecting the buffer
        // while string headers still point into it.
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
	opt := kv.Get(key)
	if opt.None() {
		return nil, false
	}
	return opt.Some().Slice(), true
}

func cacheSet(key string, val []byte) {
	kv.Set(key, cm.ToList(val))
}
