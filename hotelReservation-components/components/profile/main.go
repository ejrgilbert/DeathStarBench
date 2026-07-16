package main

import (
	"go.bytecodealliance.org/cm"

	kv "hotel-components/components/profile/cache/keyvalue/keyvalue"
	store "hotel-components/components/profile/hotel/store/profile-store"
	profapi "hotel-components/components/profile/hotel/api/profile"
)

var svc = NewService()

func main() {}

func init() {
	profapi.Exports.Init = doInit
	profapi.Exports.GetProfiles = getProfiles
}

func doInit() {
	store.Init()
}

func getProfiles(hotelIds cm.List[string]) (result cm.List[profapi.Hotel]) {
	profiles := svc.GetProfiles(hotelIds.Slice(), loadAll, cacheGet, cacheSet)

	witResult := make([]profapi.Hotel, len(profiles))
	for i, p := range profiles {
		images := make([]profapi.Image, len(p.Images))
		for k, img := range p.Images {
			images[k] = profapi.Image{URL: img.Url, Default: img.Default}
		}
		witResult[i] = profapi.Hotel{
			ID:          p.Id,
			Name:        p.Name,
			PhoneNumber: p.PhoneNumber,
			Description: p.Description,
			Addr: profapi.Address{
				StreetNumber: p.Addr.StreetNumber,
				StreetName:   p.Addr.StreetName,
				City:         p.Addr.City,
				State:        p.Addr.State,
				Country:      p.Addr.Country,
				PostalCode:   p.Addr.PostalCode,
				Lat:          p.Addr.Lat,
				Lon:          p.Addr.Lon,
			},
			Images: cm.ToList(images),
		}
	}
	result = cm.ToList(witResult)
	return
}

func loadAll() []Hotel {
	witHotels := store.LoadProfiles().Slice()
	hotels := make([]Hotel, len(witHotels))
	for i, wh := range witHotels {
		imgSlice := wh.Images.Slice()
		images := make([]Image, len(imgSlice))
		for k, img := range imgSlice {
			// bug workaround: string([]byte(s)) copies data out of the WIT-allocated buffer into
			// Go-managed heap memory, preventing the GC from collecting the buffer
			// while string headers still point into it.
			images[k] = Image{Url: string([]byte(img.URL)), Default: img.Default}
		}
		hotels[i] = Hotel{
			Id:          string([]byte(wh.ID)),
			Name:        string([]byte(wh.Name)),
			PhoneNumber: string([]byte(wh.PhoneNumber)),
			Description: string([]byte(wh.Description)),
			Addr: Address{
				StreetNumber: string([]byte(wh.Addr.StreetNumber)),
				StreetName:   string([]byte(wh.Addr.StreetName)),
				City:         string([]byte(wh.Addr.City)),
				State:        string([]byte(wh.Addr.State)),
				Country:      string([]byte(wh.Addr.Country)),
				PostalCode:   string([]byte(wh.Addr.PostalCode)),
				Lat:          wh.Addr.Lat,
				Lon:          wh.Addr.Lon,
			},
			Images: images,
		}
	}
	return hotels
}

func cacheGet(key string) ([]byte, bool) {
	opt := kv.Get(key)
	if opt.None() {
		return nil, false
	}
	return opt.Some().Slice(), true
}

func cacheSet(key string, val []byte) {
	kv.Set(key, cm.ToList(val))
}
