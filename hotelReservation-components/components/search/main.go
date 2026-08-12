package main

import (
	gcutil "hotel-components/internal/gcutil"

	"go.bytecodealliance.org/cm"

	geoapi    "hotel-components/components/search/hotel/api/geo"
	rateapi   "hotel-components/components/search/hotel/api/rate"
	searchapi "hotel-components/components/search/hotel/api/search"
)

func main() {}

func init() {
	searchapi.Exports.Nearby = nearby
}

func nearby(lat, lon float64, inDate, outDate string) cm.List[string] {
	witIds := geoapi.Nearby(lat, lon).Slice()
	ids := make([]string, len(witIds))
	for i, id := range witIds {
		ids[i] = string([]byte(id))
	}

	witPlans := rateapi.GetRates(cm.ToList(ids), inDate, outDate).Slice()

	result := make([]string, 0, len(witPlans))
	for _, rp := range witPlans {
		result = append(result, string([]byte(rp.HotelID)))
	}
	gcutil.Tick()
	return cm.ToList(result)
}
