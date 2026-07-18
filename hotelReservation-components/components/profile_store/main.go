package main

import (
	"encoding/json"

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

		images := make([]profstore.Image, len(s.Images))
		for k, img := range s.Images {
			images[k] = profstore.Image{URL: img.Url, Default: img.Default}
		}

		profiles = append(profiles, profstore.Hotel{
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
		})
	}
	allLoaded = true
}

func loadProfiles() cm.List[profstore.Hotel] {
	ensureLoaded()
	return cm.ToList(profiles)
}
