package main

import (
	"errors"
	"math"

	"github.com/hailocab/go-geoindex"
)

type Hotel struct {
	ID    string
	Lat   float64
	Lon   float64
	Rate  float64
	Price float64
}

type Requirement uint8

const (
	RequirementDistance Requirement = iota
	RequirementRate
	RequirementPrice
)

type Service struct {
	hotels map[string]Hotel
}

func NewService() *Service {
	return &Service{
		hotels: make(map[string]Hotel),
	}
}

func (s *Service) Load(hotels []Hotel) {
	s.hotels = make(map[string]Hotel, len(hotels))

	for _, h := range hotels {
		s.hotels[h.ID] = h
	}
}

func (s *Service) Recommend(
	requirement Requirement,
	lat float64,
	lon float64,
) ([]string, error) {

	switch requirement {

	case RequirementDistance:
		return s.closest(lat, lon), nil

	case RequirementRate:
		return s.bestRated(), nil

	case RequirementPrice:
		return s.cheapest(), nil

	default:
		return nil, errors.New("unknown requirement")
	}
}

func (s *Service) closest(lat, lon float64) []string {

	user := geoindex.GeoPoint{
		Plat: lat,
		Plon: lon,
	}

	best := math.MaxFloat64
	var ids []string

	for _, h := range s.hotels {

		d := float64(
			geoindex.Distance(
				&user,
				&geoindex.GeoPoint{
					Plat: h.Lat,
					Plon: h.Lon,
				},
			),
		) / 1000

		switch {
		case d < best:
			best = d
			ids = []string{h.ID}

		case d == best:
			ids = append(ids, h.ID)
		}
	}

	return ids
}

func (s *Service) bestRated() []string {

	best := -1.0
	var ids []string

	for _, h := range s.hotels {

		switch {
		case h.Rate > best:
			best = h.Rate
			ids = []string{h.ID}

		case h.Rate == best:
			ids = append(ids, h.ID)
		}
	}

	return ids
}

func (s *Service) cheapest() []string {

	best := math.MaxFloat64
	var ids []string

	for _, h := range s.hotels {

		switch {
		case h.Price < best:
			best = h.Price
			ids = []string{h.ID}

		case h.Price == best:
			ids = append(ids, h.ID)
		}
	}

	return ids
}