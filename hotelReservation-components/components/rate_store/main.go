package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	col "hotel-components/components/rate_store/host/storage/collection"
	ratestore "hotel-components/components/rate_store/hotel/rate-data/rate-store"
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
	rates     []ratestore.RatePlan
	allLoaded bool
)

func main() {}

func init() {
	ratestore.Exports.Init = doInit
	ratestore.Exports.LoadRates = loadRates
}

func doInit() {
	if col.Count() > 0 {
		return
	}

	data, err := os.ReadFile("/data/rate-seed.json")
	if err != nil {
		panic("read seed file: " + err.Error())
	}

	var seeds []seedRatePlan
	if err := json.Unmarshal(data, &seeds); err != nil {
		panic("parse seed data: " + err.Error())
	}

	docs := make([]col.Document, len(seeds))
	for i, s := range seeds {
		b, _ := json.Marshal(s)
		docs[i] = col.Document(cm.ToList(b))
	}
	col.InsertMany(cm.ToList(docs))
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	rawDocs := col.FindAll().Slice()
	for _, raw := range rawDocs {
		var s seedRatePlan
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		rates = append(rates, ratestore.RatePlan{
			HotelId: s.HotelId,
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
