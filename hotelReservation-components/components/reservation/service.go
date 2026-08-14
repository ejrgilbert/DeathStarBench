package main

import (
	"strconv"
	"time"
)

const dateFmt = "2006-01-02"

type NumberRec struct {
	HotelId      string
	NumberOfRoom uint32
}

type ReservationRec struct {
	HotelId      string
	CustomerName string
	InDate       string
	OutDate      string
	Number       uint32
}

func datePairs(inDate, outDate string) [][2]string {
	in, _ := time.Parse(dateFmt, inDate)
	out, _ := time.Parse(dateFmt, outDate)
	var pairs [][2]string
	for in.Before(out) {
		next := in.AddDate(0, 0, 1)
		pairs = append(pairs, [2]string{in.Format(dateFmt), next.Format(dateFmt)})
		in = next
	}
	return pairs
}

type Service struct{}

func NewService() *Service { return &Service{} }

// CheckAvailability mirrors the Go reservation service: per hotel, resolve the
// room capacity from cache or a targeted number lookup, then per date-night
// resolve the reserved count from cache or a targeted
// `Find({hotelId,inDate,outDate})`. It never loads whole collections.
func (s *Service) CheckAvailability(
	hotelIds []string,
	inDate, outDate string,
	roomNumber int32,
	getNumbers      func([]string) []NumberRec,
	getReservations func(string, string, string) []ReservationRec,
	cacheGetMulti   func([]string) [][]byte,
	cacheSet        func(string, []byte),
) []string {
	// Capacities: one batched cache probe for all `_cap` keys, then one batched
	// `Find({$in})` for the misses (matches native GetMulti + Find($in)).
	capKeys := make([]string, len(hotelIds))
	for i, id := range hotelIds {
		capKeys[i] = id + "_cap"
	}
	capVals := cacheGetMulti(capKeys)
	caps := make(map[string]int, len(hotelIds))
	var missedCapIds []string
	for i, id := range hotelIds {
		if capVals[i] != nil {
			n, _ := strconv.Atoi(string(capVals[i]))
			caps[id] = n
		} else {
			missedCapIds = append(missedCapIds, id)
		}
	}
	if len(missedCapIds) > 0 {
		for _, nr := range getNumbers(missedCapIds) {
			caps[nr.HotelId] = int(nr.NumberOfRoom)
			cacheSet(nr.HotelId+"_cap", []byte(strconv.Itoa(int(nr.NumberOfRoom))))
		}
	}

	// Date-night reserved counts: one batched cache probe for all (hotel, night)
	// keys, then a targeted `Find({hotelId,inDate,outDate})` per miss (native does
	// one GetMulti + per-command Find on miss).
	pairs := datePairs(inDate, outDate)
	dateKeys := make([]string, 0, len(hotelIds)*len(pairs))
	type dk struct{ id, d0, d1 string }
	dkMeta := make([]dk, 0, len(hotelIds)*len(pairs))
	for _, id := range hotelIds {
		for _, pair := range pairs {
			dateKeys = append(dateKeys, id+"_"+pair[0]+"_"+pair[1])
			dkMeta = append(dkMeta, dk{id, pair[0], pair[1]})
		}
	}
	dateVals := cacheGetMulti(dateKeys)
	counts := make(map[string]int, len(dateKeys))
	for i, key := range dateKeys {
		if dateVals[i] != nil {
			c, _ := strconv.Atoi(string(dateVals[i]))
			counts[key] = c
		} else {
			c := 0
			for _, r := range getReservations(dkMeta[i].id, dkMeta[i].d0, dkMeta[i].d1) {
				c += int(r.Number)
			}
			counts[key] = c
			cacheSet(key, []byte(strconv.Itoa(c)))
		}
	}

	var result []string
	for _, id := range hotelIds {
		cap := caps[id]
		available := true
		for _, pair := range pairs {
			key := id + "_" + pair[0] + "_" + pair[1]
			if counts[key]+int(roomNumber) > cap {
				available = false
				break
			}
		}
		if available {
			result = append(result, id)
		}
	}
	return result
}

func (s *Service) MakeReservation(
	hotelId, customerName, inDate, outDate string,
	roomNumber int32,
	getNumbers        func([]string) []NumberRec,
	getReservations   func(string, string, string) []ReservationRec,
	insertReservation func(ReservationRec),
	cacheGetMulti     func([]string) [][]byte,
	cacheGet          func(string) ([]byte, bool),
	cacheSet          func(string, []byte),
) []string {
	avail := s.CheckAvailability(
		[]string{hotelId}, inDate, outDate, roomNumber,
		getNumbers, getReservations, cacheGetMulti, cacheSet,
	)
	if len(avail) == 0 {
		return nil
	}
	for _, pair := range datePairs(inDate, outDate) {
		insertReservation(ReservationRec{
			HotelId:      hotelId,
			CustomerName: customerName,
			InDate:       pair[0],
			OutDate:      pair[1],
			Number:       uint32(roomNumber),
		})
		key := hotelId + "_" + pair[0] + "_" + pair[1]
		count := 0
		if b, ok := cacheGet(key); ok {
			count, _ = strconv.Atoi(string(b))
		}
		cacheSet(key, []byte(strconv.Itoa(count+int(roomNumber))))
	}
	return []string{hotelId}
}
