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

// svc holds the geo indices, built once (parity with the Go service's init-time
// newGeoIndex*). loadAttractionData is cached, but rebuilding the ClusteringIndex
// per request was both a perf and parity bug.
var svc *Service

func ensureSvc() *Service {
	if svc == nil {
		rests, mus, cin := loadAttractionData()
		svc = NewService()
		svc.Load(rests, mus, cin)
	}
	return svc
}

// resolveHotel does a targeted store lookup for the hotel's lat/lon, matching the
// Go attractions service's per-request `Find({"hotelId": id})`.
func resolveHotel(hotelID string) (lat, lon float64, ok bool) {
	opt := store.GetHotelPosition(hotelID)
	if opt.None() {
		return 0, 0, false
	}
	p := opt.Some()
	return p.Lat, p.Lon, true
}

func nearbyRest(hotelID string) cm.List[string] {
	lat, lon, ok := resolveHotel(hotelID)
	if !ok {
		return cm.ToList([]string{})
	}
	return cm.ToList(ensureSvc().NearbyRest(lat, lon))
}

func nearbyMus(hotelID string) cm.List[string] {
	lat, lon, ok := resolveHotel(hotelID)
	if !ok {
		return cm.ToList([]string{})
	}
	return cm.ToList(ensureSvc().NearbyMus(lat, lon))
}

func nearbyCinema(hotelID string) cm.List[string] {
	lat, lon, ok := resolveHotel(hotelID)
	if !ok {
		return cm.ToList([]string{})
	}
	return cm.ToList(ensureSvc().NearbyCinema(lat, lon))
}
