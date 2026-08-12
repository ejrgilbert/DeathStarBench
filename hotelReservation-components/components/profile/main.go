package main

import (
	gcutil "hotel-components/internal/gcutil"

	"go.bytecodealliance.org/cm"

	hkv     "hotel-components/components/profile/host/cache/keyvalue"
	store   "hotel-components/components/profile/hotel/store/profile-store"
	profapi "hotel-components/components/profile/hotel/api/profile"
)

var svc = NewService()

func main() {}

func init() {
	profapi.Exports.GetProfiles = getProfiles
}

func getProfiles(hotelIds cm.List[string]) (result cm.List[profapi.Hotel]) {
	profiles := svc.GetProfiles(hotelIds.Slice(), getOne, cacheGet, cacheSet)

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
	gcutil.Tick()
	return
}

// getOne does a targeted single-hotel lookup by id via the store's
// `get-profile` (which issues `FindOne({"id": id})`), matching the Go original.
func getOne(id string) (Hotel, bool) {
	opt := store.GetProfile(id)
	if opt.None() {
		return Hotel{}, false
	}
	return witToHotel(*opt.Some()), true
}

func witToHotel(wh store.Hotel) Hotel {
	imgSlice := wh.Images.Slice()
	images := make([]Image, len(imgSlice))
	for k, img := range imgSlice {
		images[k] = Image{Url: string([]byte(img.URL)), Default: img.Default}
	}
	return Hotel{
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

func cacheGet(key string) ([]byte, bool) {
	opt := hkv.Get(key)
	if opt.None() {
		return nil, false
	}
	return opt.Some().Slice(), true
}

func cacheSet(key string, val []byte) {
	hkv.Set(key, cm.ToList(val))
}
