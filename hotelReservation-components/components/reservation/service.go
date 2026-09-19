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

// CheckAvailability mirrors the Go reservation service's CheckAvailability on the
// search path.
//
// QUIRK REPLICATED FROM native services/reservation/server.go CheckAvailability
// (kept identical for a fair native-vs-component comparison; the behavior is the
// original's, not the port's): the original probes memcached with GetMulti for
// every hotel `_cap` key AND every (hotel, night) reserved-count key, but its
// mongo-fallback is guarded by `if err == memcache.ErrCacheMiss` — and GetMulti
// NEVER returns ErrCacheMiss (only single Get does), so that branch is dead. The
// net effect: CheckAvailability issues the (often large, duplicate-laden) batch
// of cache GETs, uses only the HITS, and does NO mongo read and NO cache set. A
// hotel stays available unless a *hit* shows it over capacity; missing keys are
// simply skipped. We reproduce exactly that so the search-path cache/DB load
// matches the original (the port previously did a mongo Find + Set per miss here,
// which the original does not).
func (s *Service) CheckAvailability(
	hotelIds []string,
	inDate, outDate string,
	roomNumber int32,
	cacheGetMulti func([]string) [][]byte,
) []string {
	// Capacities: one batched cache probe over ALL hotelIds (duplicates included,
	// matching the original's candidate set). Hits only; misses are NOT resolved
	// from mongo and NOT written back.
	capKeys := make([]string, len(hotelIds))
	for i, id := range hotelIds {
		capKeys[i] = id + "_cap"
	}
	capVals := cacheGetMulti(capKeys)
	caps := make(map[string]int, len(hotelIds))
	for i, id := range hotelIds {
		if capVals[i] != nil {
			caps[id], _ = strconv.Atoi(string(capVals[i]))
		}
	}

	// Date-night reserved counts: one batched cache probe over ALL (hotel, night)
	// keys (again duplicate-laden). Hits only; no mongo, no set on miss.
	pairs := datePairs(inDate, outDate)
	dateKeys := make([]string, 0, len(hotelIds)*len(pairs))
	for _, id := range hotelIds {
		for _, pair := range pairs {
			dateKeys = append(dateKeys, id+"_"+pair[0]+"_"+pair[1])
		}
	}
	dateVals := cacheGetMulti(dateKeys)
	counts := make(map[string]int, len(dateKeys))
	for i, key := range dateKeys {
		if dateVals[i] != nil {
			counts[key], _ = strconv.Atoi(string(dateVals[i]))
		}
	}

	// A hotel is available unless a HIT night-key shows it over capacity. Output is
	// deduplicated (the original accumulates into a per-hotel map), so duplicate
	// candidates inflate only the cache-probe count, not the result.
	seen := make(map[string]struct{}, len(hotelIds))
	var result []string
	for _, id := range hotelIds {
		if _, dup := seen[id]; dup {
			continue
		}
		seen[id] = struct{}{}
		available := true
		for _, pair := range pairs {
			key := id + "_" + pair[0] + "_" + pair[1]
			if c, hit := counts[key]; hit && c+int(roomNumber) > caps[id] {
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

// MakeReservation mirrors the Go reservation service's MakeReservation exactly,
// so the reservation-path cache/DB operation counts match native. Unlike
// CheckAvailability, it uses single-key memcached Get (whose ErrCacheMiss IS
// live), so its mongo fallback runs. The original's precise pattern, reproduced
// here: for each night do one Get of the reserved count (miss -> targeted
// `Find({hotelId,inDate,outDate})`, no set) AND one Get of the `_cap` key (miss
// -> `get-number` FindOne + Set); if any night is over capacity, bail. Only after
// every night passes does it write each night's new count (count+roomNumber) back
// ONCE (native accumulates these in a map and sets them after the check). Finally
// it inserts one reservation doc per night, with no further cache writes.
func (s *Service) MakeReservation(
	hotelId, customerName, inDate, outDate string,
	roomNumber int32,
	getNumber         func(string) (NumberRec, bool),
	getReservations   func(string, string, string) []ReservationRec,
	insertReservation func(ReservationRec),
	cacheGet          func(string) ([]byte, bool),
	cacheSet          func(string, []byte),
) []string {
	pairs := datePairs(inDate, outDate)
	capKey := hotelId + "_cap"

	// Availability check: per night, Get the reserved count (miss -> Find) and Get
	// capacity (miss -> FindOne + Set). Accumulate the post-booking count per night
	// but do NOT write it yet, matching native's memc_date_num_map.
	type pending struct {
		key string
		val int
	}
	updates := make([]pending, 0, len(pairs))
	for _, pair := range pairs {
		key := hotelId + "_" + pair[0] + "_" + pair[1]
		count := 0
		if b, ok := cacheGet(key); ok {
			count, _ = strconv.Atoi(string(b))
		} else {
			for _, r := range getReservations(hotelId, pair[0], pair[1]) {
				count += int(r.Number)
			}
		}
		updates = append(updates, pending{key: key, val: count + int(roomNumber)})

		cap := 0
		if b, ok := cacheGet(capKey); ok {
			cap, _ = strconv.Atoi(string(b))
		} else if nr, ok := getNumber(hotelId); ok {
			cap = int(nr.NumberOfRoom)
			cacheSet(capKey, []byte(strconv.Itoa(cap)))
		}
		if count+int(roomNumber) > cap {
			return nil
		}
	}

	// Check passed: write each night's new reserved count once.
	for _, u := range updates {
		cacheSet(u.key, []byte(strconv.Itoa(u.val)))
	}

	// Commit: one doc per night, no cache writes.
	for _, pair := range pairs {
		insertReservation(ReservationRec{
			HotelId:      hotelId,
			CustomerName: customerName,
			InDate:       pair[0],
			OutDate:      pair[1],
			Number:       uint32(roomNumber),
		})
	}
	return []string{hotelId}
}
