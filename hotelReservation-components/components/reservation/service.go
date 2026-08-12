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
	getNumber       func(string) (NumberRec, bool),
	getReservations func(string, string, string) []ReservationRec,
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []string {
	caps := make(map[string]int)
	for _, id := range hotelIds {
		if b, ok := cacheGet(id + "_cap"); ok {
			n, _ := strconv.Atoi(string(b))
			caps[id] = n
		} else if nr, ok := getNumber(id); ok {
			caps[id] = int(nr.NumberOfRoom)
			cacheSet(id+"_cap", []byte(strconv.Itoa(caps[id])))
		}
	}

	var result []string
	for _, id := range hotelIds {
		cap := caps[id]
		available := true
		for _, pair := range datePairs(inDate, outDate) {
			cacheKey := id + "_" + pair[0] + "_" + pair[1]
			count := 0
			if b, ok := cacheGet(cacheKey); ok {
				count, _ = strconv.Atoi(string(b))
			} else {
				for _, r := range getReservations(id, pair[0], pair[1]) {
					count += int(r.Number)
				}
				cacheSet(cacheKey, []byte(strconv.Itoa(count)))
			}
			if count+int(roomNumber) > cap {
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
	getNumber         func(string) (NumberRec, bool),
	getReservations   func(string, string, string) []ReservationRec,
	insertReservation func(ReservationRec),
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []string {
	avail := s.CheckAvailability(
		[]string{hotelId}, inDate, outDate, roomNumber,
		getNumber, getReservations, cacheGet, cacheSet,
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
