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
	revstore.Exports.GetReservations   = getReservations
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
	s := seedReservation{
		HotelId:      r.HotelID,
		CustomerName: r.CustomerName,
		InDate:       r.InDate,
		OutDate:      r.OutDate,
		Number:       r.Number,
	}
	b, _ := json.Marshal(s)
	col.InsertOne(resConn, col.Document(cm.ToList(b)))
	reservations = append(reservations, r)
}
