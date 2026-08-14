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

// GetProfiles mirrors the Go profile service: for each hotel id check the cache
// (memcached), and on a miss do a targeted `FindOne({"id": id})` via getOne and
// write the result back to the cache. It never loads the full collection.
func (s *Service) GetProfiles(
	hotelIds []string,
	getOne   func(string) (Hotel, bool),
	cacheGetMulti func([]string) [][]byte,
	cacheSet func(string, []byte),
) []Hotel {
	var result []Hotel

	// One batched cache probe for all hotels (matches native GetMulti).
	vals := cacheGetMulti(hotelIds)
	for i, id := range hotelIds {
		if vals[i] != nil {
			var h Hotel
			if err := json.Unmarshal(vals[i], &h); err == nil {
				result = append(result, h)
				continue
			}
		}
		// cache miss → targeted single-hotel lookup
		h, ok := getOne(id)
		if !ok {
			continue
		}
		if val, err := json.Marshal(h); err == nil {
			cacheSet(id, val)
		}
		result = append(result, h)
	}

	return result
}
