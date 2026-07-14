package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	col "hotel-components/components/profile_store/host/storage/collection"
	profstore "hotel-components/components/profile_store/hotel/profile-data/profile-store"
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
	profiles  []profstore.Hotel
	allLoaded bool
)

func main() {}

func init() {
	profstore.Exports.Init = doInit
	profstore.Exports.LoadProfiles = loadProfiles
}

func doInit() {
	if col.Count() > 0 {
		return
	}

	data, err := os.ReadFile("/data/profile-seed.json")
	if err != nil {
		panic("read seed file: " + err.Error())
	}

	var seeds []seedHotel
	if err := json.Unmarshal(data, &seeds); err != nil {
		panic("parse seed data: " + err.Error())
	}

	docs := make([]col.Document, len(seeds))
	for i, s := range seeds {
		b, _ := json.Marshal(s)
		docs[i] = col.Document(cm.ToList(b))
	}
	col.InsertMany(cm.ToList(docs))
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	rawDocs := col.FindAll().Slice()
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
