package main

import "encoding/json"

type Image struct {
	Url     string
	Default bool
}

type Review struct {
	ReviewId    string
	HotelId     string
	Name        string
	Rating      float32
	Description string
	Image       Image
}

type Service struct{}

func NewService() *Service { return &Service{} }

func (s *Service) GetReviews(
	hotelId string,
	loadAll  func() []Review,
	cacheGet func(string) ([]byte, bool),
	cacheSet func(string, []byte),
) []Review {
	if val, ok := cacheGet(hotelId); ok {
		var result []Review
		if err := json.Unmarshal(val, &result); err == nil {
			return result
		}
	}

	var result []Review
	for _, r := range loadAll() {
		if r.HotelId == hotelId {
			result = append(result, r)
		}
	}

	if b, err := json.Marshal(result); err == nil {
		cacheSet(hotelId, b)
	}
	return result
}
