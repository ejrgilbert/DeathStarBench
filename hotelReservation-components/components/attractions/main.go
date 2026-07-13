package main

import (
	"go.bytecodealliance.org/cm"

	store "hotel-components/components/attractions/hotel/attractions-data/attractions-store"
	attapi "hotel-components/components/attractions/hotel/attractions/attractions"
)

var svc = NewService()

func main() {}

func init() {
	attapi.Exports.Init = doInit
	attapi.Exports.NearbyRest = nearbyRest
	attapi.Exports.NearbyMus = nearbyMus
	attapi.Exports.NearbyCinema = nearbyCinema
}

func doInit() {
	store.Init()

	witRests := store.LoadRestaurants().Slice()
	rests := make([]Restaurant, len(witRests))
	for i, r := range witRests {
		rests[i] = Restaurant{id: r.ID, plat: r.Lat, plon: r.Lon}
	}

	witMus := store.LoadMuseums().Slice()
	mus := make([]Museum, len(witMus))
	for i, m := range witMus {
		mus[i] = Museum{id: m.ID, plat: m.Lat, plon: m.Lon}
	}

	witCin := store.LoadCinemas().Slice()
	cin := make([]Cinema, len(witCin))
	for i, c := range witCin {
		cin[i] = Cinema{id: c.ID, plat: c.Lat, plon: c.Lon}
	}

	svc.Load(rests, mus, cin)
}

func loadHotels() []Hotel {
	witHotels := store.LoadHotelPositions().Slice()
	hotels := make([]Hotel, len(witHotels))
	for i, h := range witHotels {
		hotels[i] = Hotel{id: h.ID, plat: h.Lat, plon: h.Lon}
	}
	return hotels
}

func nearbyRest(hotelID string) cm.List[string] {
	ids, _ := svc.NearbyRest(loadHotels(), hotelID)
	return cm.ToList(ids)
}

func nearbyMus(hotelID string) cm.List[string] {
	ids, _ := svc.NearbyMus(loadHotels(), hotelID)
	return cm.ToList(ids)
}

func nearbyCinema(hotelID string) cm.List[string] {
	ids, _ := svc.NearbyCinema(loadHotels(), hotelID)
	return cm.ToList(ids)
}
