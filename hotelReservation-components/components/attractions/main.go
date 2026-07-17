package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	hkv    "hotel-components/components/attractions/host/cache/keyvalue"
	store  "hotel-components/components/attractions/hotel/store/attractions-store"
	attapi "hotel-components/components/attractions/hotel/api/attractions"
)

// cachedGeoItem is a JSON-serializable form for the geo-indexed types with unexported fields.
type cachedGeoItem struct {
	ID   string  `json:"id"`
	Plat float64 `json:"plat"`
	Plon float64 `json:"plon"`
}

func main() {}

func init() {
	attapi.Exports.NearbyRest   = nearbyRest
	attapi.Exports.NearbyMus    = nearbyMus
	attapi.Exports.NearbyCinema = nearbyCinema
}

func loadAttractionData() ([]Restaurant, []Museum, []Cinema) {
	var rests []Restaurant
	var mus []Museum
	var cin []Cinema

	if opt := hkv.Get("att:rests"); !opt.None() {
		var c []cachedGeoItem
		if err := json.Unmarshal(opt.Some().Slice(), &c); err == nil {
			rests = make([]Restaurant, len(c))
			for i, v := range c {
				rests[i] = Restaurant{id: v.ID, plat: v.Plat, plon: v.Plon}
			}
		}
	}
	if opt := hkv.Get("att:mus"); !opt.None() {
		var c []cachedGeoItem
		if err := json.Unmarshal(opt.Some().Slice(), &c); err == nil {
			mus = make([]Museum, len(c))
			for i, v := range c {
				mus[i] = Museum{id: v.ID, plat: v.Plat, plon: v.Plon}
			}
		}
	}
	if opt := hkv.Get("att:cin"); !opt.None() {
		var c []cachedGeoItem
		if err := json.Unmarshal(opt.Some().Slice(), &c); err == nil {
			cin = make([]Cinema, len(c))
			for i, v := range c {
				cin[i] = Cinema{id: v.ID, plat: v.Plat, plon: v.Plon}
			}
		}
	}

	if rests != nil && mus != nil && cin != nil {
		return rests, mus, cin
	}

	witRests := store.LoadRestaurants().Slice()
	rests = make([]Restaurant, len(witRests))
	cr := make([]cachedGeoItem, len(witRests))
	for i, r := range witRests {
		rests[i] = Restaurant{id: string([]byte(r.ID)), plat: r.Lat, plon: r.Lon}
		cr[i] = cachedGeoItem{ID: rests[i].id, Plat: r.Lat, Plon: r.Lon}
	}

	witMus := store.LoadMuseums().Slice()
	mus = make([]Museum, len(witMus))
	cm2 := make([]cachedGeoItem, len(witMus))
	for i, m := range witMus {
		mus[i] = Museum{id: string([]byte(m.ID)), plat: m.Lat, plon: m.Lon}
		cm2[i] = cachedGeoItem{ID: mus[i].id, Plat: m.Lat, Plon: m.Lon}
	}

	witCin := store.LoadCinemas().Slice()
	cin = make([]Cinema, len(witCin))
	cc := make([]cachedGeoItem, len(witCin))
	for i, c := range witCin {
		cin[i] = Cinema{id: string([]byte(c.ID)), plat: c.Lat, plon: c.Lon}
		cc[i] = cachedGeoItem{ID: cin[i].id, Plat: c.Lat, Plon: c.Lon}
	}

	if b, err := json.Marshal(cr); err == nil {
		hkv.Set("att:rests", cm.ToList(b))
	}
	if b, err := json.Marshal(cm2); err == nil {
		hkv.Set("att:mus", cm.ToList(b))
	}
	if b, err := json.Marshal(cc); err == nil {
		hkv.Set("att:cin", cm.ToList(b))
	}
	return rests, mus, cin
}

func loadHotels() []Hotel {
	const key = "att:hotels"
	if opt := hkv.Get(key); !opt.None() {
		var c []cachedGeoItem
		if err := json.Unmarshal(opt.Some().Slice(), &c); err == nil {
			hotels := make([]Hotel, len(c))
			for i, v := range c {
				hotels[i] = Hotel{id: v.ID, plat: v.Plat, plon: v.Plon}
			}
			return hotels
		}
	}
	witHotels := store.LoadHotelPositions().Slice()
	hotels := make([]Hotel, len(witHotels))
	cached := make([]cachedGeoItem, len(witHotels))
	for i, h := range witHotels {
		hotels[i] = Hotel{id: string([]byte(h.ID)), plat: h.Lat, plon: h.Lon}
		cached[i] = cachedGeoItem{ID: hotels[i].id, Plat: h.Lat, Plon: h.Lon}
	}
	if b, err := json.Marshal(cached); err == nil {
		hkv.Set(key, cm.ToList(b))
	}
	return hotels
}

func buildSvc() *Service {
	rests, mus, cin := loadAttractionData()
	svc := NewService()
	svc.Load(rests, mus, cin)
	return svc
}

func nearbyRest(hotelID string) cm.List[string] {
	ids, _ := buildSvc().NearbyRest(loadHotels(), hotelID)
	return cm.ToList(ids)
}

func nearbyMus(hotelID string) cm.List[string] {
	ids, _ := buildSvc().NearbyMus(loadHotels(), hotelID)
	return cm.ToList(ids)
}

func nearbyCinema(hotelID string) cm.List[string] {
	ids, _ := buildSvc().NearbyCinema(loadHotels(), hotelID)
	return cm.ToList(ids)
}
