package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	ncol     "hotel-components/components/reservation_store/hotel/store/numbers-col"
	rcol     "hotel-components/components/reservation_store/hotel/store/reservations-col"
	revstore "hotel-components/components/reservation_store/hotel/store/reservation-store"
)

type seedNumber struct {
	HotelId      string `json:"hotelId"`
	NumberOfRoom uint32 `json:"numberOfRoom"`
}

type seedReservation struct {
	HotelId      string `json:"hotelId"`
	CustomerName string `json:"customerName"`
	InDate       string `json:"inDate"`
	OutDate      string `json:"outDate"`
	Number       uint32 `json:"number"`
}

var (
	numbers      []revstore.NumberRec
	reservations []revstore.ReservationRec
	numsLoaded   bool
	resLoaded    bool
)

func main() {}

func init() {
	revstore.Exports.Init              = doInit
	revstore.Exports.LoadNumbers       = loadNumbers
	revstore.Exports.LoadReservations  = loadReservations
	revstore.Exports.InsertReservation = doInsertReservation
}

func doInit() {
	if ncol.Count() == 0 {
		data, err := os.ReadFile("/data/reservation-numbers-seed.json")
		if err != nil {
			panic("read numbers seed: " + err.Error())
		}
		var seeds []seedNumber
		if err := json.Unmarshal(data, &seeds); err != nil {
			panic("parse numbers seed: " + err.Error())
		}
		docs := make([]ncol.Document, len(seeds))
		for i, s := range seeds {
			b, _ := json.Marshal(s)
			docs[i] = ncol.Document(cm.ToList(b))
		}
		ncol.InsertMany(cm.ToList(docs))
	}

	existing := rcol.FindAll().Slice()
	if len(existing) == 0 {
		data, err := os.ReadFile("/data/reservation-reservations-seed.json")
		if err != nil {
			panic("read reservations seed: " + err.Error())
		}
		var seeds []seedReservation
		if err := json.Unmarshal(data, &seeds); err != nil {
			panic("parse reservations seed: " + err.Error())
		}
		for _, s := range seeds {
			b, _ := json.Marshal(s)
			rcol.InsertOne(rcol.Document(cm.ToList(b)))
		}
	}
}

func ensureNumbers() {
	if numsLoaded {
		return
	}
	rawDocs := ncol.FindAll().Slice()
	for _, raw := range rawDocs {
		var s seedNumber
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		numbers = append(numbers, revstore.NumberRec{
			HotelID:      string([]byte(s.HotelId)),
			NumberOfRoom: s.NumberOfRoom,
		})
	}
	numsLoaded = true
}

func ensureReservations() {
	if resLoaded {
		return
	}
	rawDocs := rcol.FindAll().Slice()
	for _, raw := range rawDocs {
		var s seedReservation
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		reservations = append(reservations, revstore.ReservationRec{
			HotelID:      string([]byte(s.HotelId)),
			CustomerName: string([]byte(s.CustomerName)),
			InDate:       string([]byte(s.InDate)),
			OutDate:      string([]byte(s.OutDate)),
			Number:       s.Number,
		})
	}
	resLoaded = true
}

func loadNumbers() cm.List[revstore.NumberRec] {
	ensureNumbers()
	return cm.ToList(numbers)
}

func loadReservations() cm.List[revstore.ReservationRec] {
	ensureReservations()
	return cm.ToList(reservations)
}

func doInsertReservation(r revstore.ReservationRec) {
	s := seedReservation{
		HotelId:      r.HotelID,
		CustomerName: r.CustomerName,
		InDate:       r.InDate,
		OutDate:      r.OutDate,
		Number:       r.Number,
	}
	b, _ := json.Marshal(s)
	rcol.InsertOne(rcol.Document(cm.ToList(b)))
	reservations = append(reservations, r)
}
