package main

import (
	"go.bytecodealliance.org/cm"

	kv "hotel-components/components/rate/host/cache/keyvalue"
	store "hotel-components/components/rate/hotel/rate-data/rate-store"
	rateapi "hotel-components/components/rate/hotel/rate/rate"
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

func getRates(hotelIds []string, inDate, outDate string) cm.List[rateapi.RatePlan] {
	result := svc.GetRates(hotelIds, loadAll, cacheGet, cacheSet)

	witResult := make([]rateapi.RatePlan, len(result))
	for i, p := range result {
		witResult[i] = rateapi.RatePlan{
			HotelId: p.HotelId,
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
	return cm.ToList(witResult)
}

func loadAll() []RatePlan {
	witPlans := store.LoadRates().Slice()
	plans := make([]RatePlan, len(witPlans))
	for i, wp := range witPlans {
		plans[i] = RatePlan{
			HotelId: wp.HotelId,
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

func cacheGet(key string) ([]byte, bool) {
	opt := kv.Get(key)
	if opt.None() != nil {
		return nil, false
	}
	return opt.Some().Slice(), true
}

func cacheSet(key string, val []byte) {
	kv.Set(key, cm.ToList(val))
}
