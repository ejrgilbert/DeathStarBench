package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	col       "hotel-components/components/rate_store/host/storage/collection"
	ratestore "hotel-components/components/rate_store/hotel/store/rate-store"
)

type seedRoomType struct {
	BookableRate       float64 `json:"bookableRate"`
	Code               string  `json:"code"`
	RoomDescription    string  `json:"roomDescription"`
	TotalRate          float64 `json:"totalRate"`
	TotalRateInclusive float64 `json:"totalRateInclusive"`
}

type seedRatePlan struct {
	HotelId  string       `json:"hotelId"`
	Code     string       `json:"code"`
	InDate   string       `json:"inDate"`
	OutDate  string       `json:"outDate"`
	RoomType seedRoomType `json:"roomType"`
}

var (
	conn      col.Connection
	connOpen  bool
	rates     []ratestore.RatePlan
	allLoaded bool
)

func main() {}

func init() {
	ratestore.Exports.LoadRates = loadRates
}

func ensureConn() {
	if connOpen {
		return
	}
	conn = col.ConnectionOpen("rates")
	connOpen = true
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	ensureConn()
	rawDocs := col.FindAll(conn).Slice()
	for _, raw := range rawDocs {
		var s seedRatePlan
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		rates = append(rates, ratestore.RatePlan{
			HotelID: s.HotelId,
			Code:    s.Code,
			InDate:  s.InDate,
			OutDate: s.OutDate,
			RoomType: ratestore.RoomType{
				BookableRate:       s.RoomType.BookableRate,
				Code:               s.RoomType.Code,
				RoomDescription:    s.RoomType.RoomDescription,
				TotalRate:          s.RoomType.TotalRate,
				TotalRateInclusive: s.RoomType.TotalRateInclusive,
			},
		})
	}
	allLoaded = true
}

func loadRates() cm.List[ratestore.RatePlan] {
	ensureLoaded()
	return cm.ToList(rates)
}
