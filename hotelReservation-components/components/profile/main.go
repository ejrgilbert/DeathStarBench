package main

import (
	gcutil "hotel-components/internal/gcutil"

	"go.bytecodealliance.org/cm"

	hkv     "hotel-components/components/profile/cache/keyvalue/keyvalue"
	store   "hotel-components/components/profile/hotel/store/profile-store"
	profapi "hotel-components/components/profile/hotel/api/profile"
)

var svc = NewService()

func main() {}

func init() {
	profapi.Exports.GetProfiles = getProfiles
}

func getProfiles(hotelIds cm.List[string]) (result cm.List[profapi.Hotel]) {
	profiles := svc.GetProfiles(hotelIds.Slice(), getOne, cacheGetMulti, cacheSet)

	witResult := make([]profapi.Hotel, len(profiles))
	for i, p := range profiles {
		images := make([]profapi.Image, len(p.Images))
		for k, img := range p.Images {
			images[k] = profapi.Image{URL: string([]byte(img.Url)), Default: img.Default}
		}
		witResult[i] = profapi.Hotel{
			ID:          string([]byte(p.Id)),
			Name:        string([]byte(p.Name)),
			PhoneNumber: string([]byte(p.PhoneNumber)),
			Description: string([]byte(p.Description)),
			Addr: profapi.Address{
				StreetNumber: string([]byte(p.Addr.StreetNumber)),
				StreetName:   string([]byte(p.Addr.StreetName)),
				City:         string([]byte(p.Addr.City)),
				State:        string([]byte(p.Addr.State)),
				Country:      string([]byte(p.Addr.Country)),
				PostalCode:   string([]byte(p.Addr.PostalCode)),
				Lat:          p.Addr.Lat,
				Lon:          p.Addr.Lon,
			},
			Images: cm.ToList(images),
		}
	}

	gcutil.Tick()
	result = cm.ToList(witResult)
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

// cacheNS namespaces this service's cache keys (shared host cache in ABI mode).
const cacheNS = "profile:"

// cacheGetMulti probes all keys in one batched call (native memcached GetMulti).
// The returned slice is parallel to keys; a nil entry is a cache miss.
func cacheGetMulti(keys []string) [][]byte {
	nsKeys := make([]string, len(keys))
	for i, k := range keys {
		nsKeys[i] = cacheNS + k
	}
	res := hkv.GetMulti(cm.ToList(nsKeys)).Slice()
	out := make([][]byte, len(keys))
	for i := range keys {
		if i < len(res) && !res[i].None() {
			out[i] = res[i].Some().Slice()
		}
	}
	return out
}

func cacheSet(key string, val []byte) {
	hkv.Set(cacheNS+key, cm.ToList(val))
}
