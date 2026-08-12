package main

import (
	gcutil "hotel-components/internal/gcutil"

	"go.bytecodealliance.org/cm"

	hkv     "hotel-components/components/reservation/host/cache/keyvalue"
	store   "hotel-components/components/reservation/hotel/store/reservation-store"
	resapi  "hotel-components/components/reservation/hotel/api/reservation"
)

var svc = NewService()

func main() {}

func init() {
	resapi.Exports.CheckAvailability = checkAvailability
	resapi.Exports.MakeReservation   = makeReservation
}

func checkAvailability(hotelIds cm.List[string], inDate, outDate string, roomNumber int32) cm.List[string] {
	rawIds := hotelIds.Slice()
	ids := make([]string, len(rawIds))
	for i, id := range rawIds {
		ids[i] = string([]byte(id))
	}
	result := svc.CheckAvailability(
		ids,
		string([]byte(inDate)), string([]byte(outDate)),
		roomNumber,
		getNumber, getReservations, cacheGet, cacheSet,
	)
	gcutil.Tick()
	return cm.ToList(result)
}

func makeReservation(hotelID, customerName, inDate, outDate string, roomNumber int32) cm.List[string] {
	result := svc.MakeReservation(
		string([]byte(hotelID)),
		string([]byte(customerName)),
		string([]byte(inDate)),
		string([]byte(outDate)),
		roomNumber,
		getNumber, getReservations, doInsertReservation, cacheGet, cacheSet,
	)
	gcutil.Tick()
	return cm.ToList(result)
}

// getNumber does a targeted room-capacity lookup by hotel id via the store's
// `get-number` (`FindOne({"hotelId": id})`), matching the Go original.
func getNumber(id string) (NumberRec, bool) {
	opt := store.GetNumber(id)
	if opt.None() {
		return NumberRec{}, false
	}
	n := *opt.Some()
	return NumberRec{HotelId: string([]byte(n.HotelID)), NumberOfRoom: n.NumberOfRoom}, true
}

// getReservations does a targeted `Find({"hotelId","inDate","outDate"})` via the
// store's `get-reservations`, matching the Go original.
func getReservations(id, inDate, outDate string) []ReservationRec {
	witRecs := store.GetReservations(id, inDate, outDate).Slice()
	result := make([]ReservationRec, len(witRecs))
	for i, r := range witRecs {
		result[i] = ReservationRec{
			HotelId:      string([]byte(r.HotelID)),
			CustomerName: string([]byte(r.CustomerName)),
			InDate:       string([]byte(r.InDate)),
			OutDate:      string([]byte(r.OutDate)),
			Number:       r.Number,
		}
	}
	return result
}

func doInsertReservation(r ReservationRec) {
	store.InsertReservation(store.ReservationRec{
		HotelID:      r.HotelId,
		CustomerName: r.CustomerName,
		InDate:       r.InDate,
		OutDate:      r.OutDate,
		Number:       r.Number,
	})
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
