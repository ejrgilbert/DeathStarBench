package main

import (
	"github.com/hailocab/go-geoindex"
)

const (
	maxSearchRadius  = 10
	maxSearchResults = 5
)

type Point struct {
	id   string
	plat float64
	plon float64
}
func (p *Point) Id() string   { return p.id }
func (p *Point) Lat() float64 { return p.plat }
func (p *Point) Lon() float64 { return p.plon }

type Service struct {
	points *geoindex.ClusteringIndex
}

func NewService() *Service {
	return &Service{
		points: geoindex.NewClusteringIndex(),
	}
}

func (s *Service) Load(ps []Point) {
	for _, point := range ps {
		s.points.Add(&point)
	}
}

// Nearby returns all hotels within a given distance.
func (s *Service) Nearby(lat float64, lon float64) ([]string, error) {
	center := &geoindex.GeoPoint{Plat: lat, Plon: lon}
    	pts := s.points.KNearest(center, maxSearchResults, geoindex.Km(maxSearchRadius), func(geoindex.Point) bool { return true })
    	ids := make([]string, len(pts))
    	for i, p := range pts {
    		ids[i] = p.Id()
    	}
    	return ids, nil
}
