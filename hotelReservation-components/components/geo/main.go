package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	hkv    "hotel-components/components/geo/host/cache/keyvalue"
	store  "hotel-components/components/geo/hotel/store/geo-store"
	attapi "hotel-components/components/geo/hotel/api/geo"
)

// cachedPoint is a JSON-serializable mirror of Point (which has unexported fields).
type cachedPoint struct {
	ID   string  `json:"id"`
	Plat float64 `json:"plat"`
	Plon float64 `json:"plon"`
}

func main() {}

func init() {
	attapi.Exports.Nearby = nearby
}

func loadGeo() []Point {
	const key = "geo:points"
	if opt := hkv.Get(key); !opt.None() {
		var cached []cachedPoint
		if err := json.Unmarshal(opt.Some().Slice(), &cached); err == nil {
			points := make([]Point, len(cached))
			for i, c := range cached {
				points[i] = Point{id: c.ID, plat: c.Plat, plon: c.Plon}
			}
			return points
		}
	}
	witGeo := store.LoadGeo().Slice()
	points := make([]Point, len(witGeo))
	cached := make([]cachedPoint, len(witGeo))
	for i, r := range witGeo {
		points[i] = Point{id: string([]byte(r.ID)), plat: r.Lat, plon: r.Lon}
		cached[i] = cachedPoint{ID: points[i].id, Plat: r.Lat, Plon: r.Lon}
	}
	if b, err := json.Marshal(cached); err == nil {
		hkv.Set(key, cm.ToList(b))
	}
	return points
}

func nearby(lat float64, lon float64) cm.List[string] {
	svc := NewService()
	svc.Load(loadGeo())
	ids, _ := svc.Nearby(lat, lon)
	return cm.ToList(ids)
}
