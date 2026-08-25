package main

import (
	"encoding/json"
	"fmt"

	"go.bytecodealliance.org/cm"

	col      "hotel-components/components/reservation_store/host/storage/collection"
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
	numConn      col.Connection
	resConn      col.Connection
	connOpen     bool
	numbers      []revstore.NumberRec
	reservations []revstore.ReservationRec
	numsLoaded   bool
	resLoaded    bool
)

func main() {}

func init() {
	revstore.Exports.LoadNumbers       = loadNumbers
	revstore.Exports.LoadReservations  = loadReservations
	revstore.Exports.InsertReservation = doInsertReservation
	revstore.Exports.GetNumber         = getNumber
	revstore.Exports.GetNumbers        = getNumbers
	revstore.Exports.GetReservations   = getReservations
}

// getNumbers does a batched `Find({"hotelId": {"$in": ids}})` on the number
// collection, matching the Go reservation service's cap-miss query.
func getNumbers(ids cm.List[string]) cm.List[revstore.NumberRec] {
	ensureConn()
	rawIds := ids.Slice()
	strs := make([]string, len(rawIds))
	for i, id := range rawIds {
		strs[i] = string([]byte(id))
	}
	idsJSON, _ := json.Marshal(strs)
	filter := fmt.Sprintf(`{"hotelId":{"$in":%s}}`, string(idsJSON))
	rawDocs := col.Find(numConn, col.Document(cm.ToList([]uint8(filter)))).Slice()
	recs := make([]revstore.NumberRec, 0, len(rawDocs))
	for _, raw := range rawDocs {
		var s seedNumber
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		recs = append(recs, revstore.NumberRec{
			HotelID:      string([]byte(s.HotelId)),
			NumberOfRoom: s.NumberOfRoom,
		})
	}
	return cm.ToList(recs)
}

// getNumber does a targeted `FindOne({"hotelId": id})` on the number collection.
func getNumber(hotelID string) cm.Option[revstore.NumberRec] {
	ensureConn()
	filter := fmt.Sprintf(`{"hotelId":%q}`, hotelID)
	opt := col.FindOne(numConn, col.Document(cm.ToList([]uint8(filter))))
	if opt.None() {
		return cm.None[revstore.NumberRec]()
	}
	var s seedNumber
	json.Unmarshal(cm.List[uint8](*opt.Some()).Slice(), &s)
	return cm.Some(revstore.NumberRec{HotelID: string([]byte(s.HotelId)), NumberOfRoom: s.NumberOfRoom})
}

// getReservations does a targeted `Find({"hotelId","inDate","outDate"})`.
func getReservations(hotelID, inDate, outDate string) cm.List[revstore.ReservationRec] {
	ensureConn()
	filter := fmt.Sprintf(`{"hotelId":%q,"inDate":%q,"outDate":%q}`, hotelID, inDate, outDate)
	rawDocs := col.Find(resConn, col.Document(cm.ToList([]uint8(filter)))).Slice()
	recs := make([]revstore.ReservationRec, 0, len(rawDocs))
	for _, raw := range rawDocs {
		var s seedReservation
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		recs = append(recs, revstore.ReservationRec{
			HotelID:      string([]byte(s.HotelId)),
			CustomerName: string([]byte(s.CustomerName)),
			InDate:       string([]byte(s.InDate)),
			OutDate:      string([]byte(s.OutDate)),
			Number:       s.Number,
		})
	}
	return cm.ToList(recs)
}

func ensureConn() {
	if connOpen {
		return
	}
	numConn = col.ConnectionOpen("number")
	resConn = col.ConnectionOpen("reservation")
	connOpen = true
}

func ensureNumbers() {
	if numsLoaded {
		return
	}
	ensureConn()
	rawDocs := col.FindAll(numConn).Slice()
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
	ensureConn()
	rawDocs := col.FindAll(resConn).Slice()
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
	ensureConn()
	// Aggregate per (hotelId,inDate,outDate) instead of appending a row per
	// write: $inc the reserved-room count into a single doc (created on first
	// write with the customer name). This keeps getReservations' result set at
	// O(1) per key — availability only needs the summed count — so the guest
	// never lifts an unbounded cross-boundary list (which tips TinyGo's on-full
	// GC mid-call and corrupts the buffer). Mirrors the native UpdateOne+$inc.
	// Build the filter/update with json.Marshal (not Sprintf %q): %q emits
	// Go-style escapes (\x.., \U..) that are not valid JSON, which the host's
	// serde_json rejected ("invalid escape").
	filter, _ := json.Marshal(map[string]string{
		"hotelId": r.HotelID,
		"inDate":  r.InDate,
		"outDate": r.OutDate,
	})
	update, _ := json.Marshal(map[string]interface{}{
		"$inc":         map[string]uint32{"number": r.Number},
		"$setOnInsert": map[string]string{"customerName": r.CustomerName},
	})
	col.UpdateOne(resConn,
		col.Document(cm.ToList(filter)),
		col.Document(cm.ToList(update)),
		true)
	reservations = append(reservations, r)
}
