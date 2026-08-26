package main

import (

	"go.bytecodealliance.org/cm"

	hkv     "hotel-components/components/reservation/cache/keyvalue/keyvalue"
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
		ids[i] = id
	}
	result := svc.CheckAvailability(
		ids,
		inDate, outDate,
		roomNumber,
		getNumbers, getReservations, cacheGetMulti, cacheSet,
	)
	return cm.ToList(result)
}

func makeReservation(hotelID, customerName, inDate, outDate string, roomNumber int32) cm.List[string] {
	result := svc.MakeReservation(
		hotelID,
		customerName,
		inDate,
		outDate,
		roomNumber,
		getNumbers, getReservations, doInsertReservation, cacheGetMulti, cacheGet, cacheSet,
	)
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
	return NumberRec{HotelId: n.HotelID, NumberOfRoom: n.NumberOfRoom}, true
}

// getNumbers does a batched capacity lookup via the store's `get-numbers`
// (`Find({hotelId:{$in}})`), matching the Go reservation cap-miss query.
func getNumbers(ids []string) []NumberRec {
	witRecs := store.GetNumbers(cm.ToList(ids)).Slice()
	out := make([]NumberRec, len(witRecs))
	for i, n := range witRecs {
		out[i] = NumberRec{HotelId: n.HotelID, NumberOfRoom: n.NumberOfRoom}
	}
	return out
}

// getReservations does a targeted `Find({"hotelId","inDate","outDate"})` via the
// store's `get-reservations`, matching the Go original.
func getReservations(id, inDate, outDate string) []ReservationRec {
	witRecs := store.GetReservations(id, inDate, outDate).Slice()
	result := make([]ReservationRec, len(witRecs))
	for i, r := range witRecs {
		result[i] = ReservationRec{
			HotelId:      r.HotelID,
			CustomerName: r.CustomerName,
			InDate:       r.InDate,
			OutDate:      r.OutDate,
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

// cacheNS namespaces this service's cache keys (shared host cache in ABI mode).
const cacheNS = "resv:"

func cacheGet(key string) ([]byte, bool) {
	opt := hkv.Get(cacheNS + key)
	if opt.None() {
		return nil, false
	}
	return opt.Some().Slice(), true
}

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
