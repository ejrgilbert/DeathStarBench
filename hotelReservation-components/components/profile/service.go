package main

import "encoding/json"

type Address struct {
	StreetNumber string
	StreetName   string
	City         string
	State        string
	Country      string
	PostalCode   string
	Lat          float64
	Lon          float64
}

type Image struct {
	Url     string
	Default bool
}

type Hotel struct {
	Id          string
	Name        string
	PhoneNumber string
	Description string
	Addr        Address
	Images      []Image
}

type Service struct{}

func NewService() *Service { return &Service{} }

func (s *Service) GetProfiles(
	hotelIds []string,
	loadAll  func() []Hotel,
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []Hotel {
	var result []Hotel
	var missed []string

	for _, id := range hotelIds {
		if val, ok := cacheGet(id); ok {
			var h Hotel
			if err := json.Unmarshal(val, &h); err == nil {
				result = append(result, h)
				continue
			}
		}
		missed = append(missed, id)
	}

	if len(missed) > 0 {
		byHotel := make(map[string]Hotel)
		for _, h := range loadAll() {
			byHotel[h.Id] = h
		}

		for _, h := range byHotel {
			if val, err := json.Marshal(h); err != nil {
				println("profile: failed to marshal hotel", h.Id, ":", err.Error())
			} else {
				cacheSet(h.Id, val)
			}
		}
		for _, id := range missed {
			if h, ok := byHotel[id]; ok {
				result = append(result, h)
			}
		}
	}

	return result
}
