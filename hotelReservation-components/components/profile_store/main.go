package main

import (
	"encoding/json"
	"fmt"

	"go.bytecodealliance.org/cm"

	col       "hotel-components/components/profile_store/host/storage/collection"
	profstore "hotel-components/components/profile_store/hotel/store/profile-store"
)

type seedAddress struct {
	StreetNumber string  `json:"streetNumber"`
	StreetName   string  `json:"streetName"`
	City         string  `json:"city"`
	State        string  `json:"state"`
	Country      string  `json:"country"`
	PostalCode   string  `json:"postalCode"`
	Lat          float64 `json:"lat"`
	Lon          float64 `json:"lon"`
}

type seedImage struct {
	Url     string `json:"url"`
	Default bool   `json:"default"`
}

type seedHotel struct {
	Id          string      `json:"id"`
	Name        string      `json:"name"`
	PhoneNumber string      `json:"phoneNumber"`
	Description string      `json:"description"`
	Address     seedAddress `json:"address"`
	Images      []seedImage `json:"images"`
}

var (
	conn      col.Connection
	connOpen  bool
	profiles  []profstore.Hotel
	allLoaded bool
)

func main() {}

func init() {
	profstore.Exports.LoadProfiles = loadProfiles
	profstore.Exports.GetProfile = getProfile
}

func toHotel(s seedHotel) profstore.Hotel {
	images := make([]profstore.Image, len(s.Images))
	for k, img := range s.Images {
		images[k] = profstore.Image{URL: img.Url, Default: img.Default}
	}
	return profstore.Hotel{
		ID:          s.Id,
		Name:        s.Name,
		PhoneNumber: s.PhoneNumber,
		Description: s.Description,
		Addr: profstore.Address{
			StreetNumber: s.Address.StreetNumber,
			StreetName:   s.Address.StreetName,
			City:         s.Address.City,
			State:        s.Address.State,
			Country:      s.Address.Country,
			PostalCode:   s.Address.PostalCode,
			Lat:          s.Address.Lat,
			Lon:          s.Address.Lon,
		},
		Images: cm.ToList(images),
	}
}

func ensureConn() {
	if connOpen {
		return
	}
	conn = col.ConnectionOpen("profiles")
	connOpen = true
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	ensureConn()
	rawDocs := col.FindAll(conn).Slice()
	for _, raw := range rawDocs {
		var s seedHotel
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		profiles = append(profiles, toHotel(s))
	}
	allLoaded = true
}

func loadProfiles() cm.List[profstore.Hotel] {
	ensureLoaded()
	return cm.ToList(profiles)
}

// getProfile does a targeted single-document lookup by id, matching the Go
// profile service's `collection.FindOne({"id": hotelId})`.
func getProfile(id string) cm.Option[profstore.Hotel] {
	ensureConn()
	filter := fmt.Sprintf(`{"id":%q}`, id)
	opt := col.FindOne(conn, col.Document(cm.ToList([]uint8(filter))))
	if opt.None() {
		return cm.None[profstore.Hotel]()
	}
	var s seedHotel
	json.Unmarshal(cm.List[uint8](*opt.Some()).Slice(), &s)
	return cm.Some(toHotel(s))
}
