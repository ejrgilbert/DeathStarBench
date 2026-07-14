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

func (s *Service) CheckAvailability(
	hotelIds []string,
	inDate, outDate string,
	roomNumber int32,
	loadNumbers      func() []NumberRec,
	loadReservations func() []ReservationRec,
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []string {
	caps := make(map[string]int)
	var missIds []string
	for _, id := range hotelIds {
		if b, ok := cacheGet(id + "_cap"); ok {
			n, _ := strconv.Atoi(string(b))
			caps[id] = n
		} else {
			missIds = append(missIds, id)
		}
	}
	if len(missIds) > 0 {
		numMap := make(map[string]int)
		for _, nr := range loadNumbers() {
			numMap[nr.HotelId] = int(nr.NumberOfRoom)
		}
		for _, id := range missIds {
			if cap, ok := numMap[id]; ok {
				caps[id] = cap
				cacheSet(id+"_cap", []byte(strconv.Itoa(cap)))
			}
		}
	}

	var resSlice []ReservationRec
	resLoaded := false

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
				if !resLoaded {
					resSlice = loadReservations()
					resLoaded = true
				}
				for _, r := range resSlice {
					if r.HotelId == id && r.InDate == pair[0] && r.OutDate == pair[1] {
						count += int(r.Number)
					}
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
	loadNumbers       func() []NumberRec,
	loadReservations  func() []ReservationRec,
	insertReservation func(ReservationRec),
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []string {
	avail := s.CheckAvailability(
		[]string{hotelId}, inDate, outDate, roomNumber,
		loadNumbers, loadReservations, cacheGet, cacheSet,
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
