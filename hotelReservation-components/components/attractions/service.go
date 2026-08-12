package main

import (
	"github.com/hailocab/go-geoindex"
)

const (
	maxSearchRadius  = 10
	maxSearchResults = 5
)

type Restaurant struct {
	id   string
	plat float64
	plon float64
}

func (r *Restaurant) Id() string   { return r.id }
func (r *Restaurant) Lat() float64 { return r.plat }
func (r *Restaurant) Lon() float64 { return r.plon }

type Museum struct {
	id   string
	plat float64
	plon float64
}

func (m *Museum) Id() string   { return m.id }
func (m *Museum) Lat() float64 { return m.plat }
func (m *Museum) Lon() float64 { return m.plon }

type Cinema struct {
	id   string
	plat float64
	plon float64
}

func (c *Cinema) Id() string   { return c.id }
func (c *Cinema) Lat() float64 { return c.plat }
func (c *Cinema) Lon() float64 { return c.plon }

// Service mirrors the original: attraction geo indices are built once at init;
// hotel lat/lon is resolved from the store on every request (no caching).
type Service struct {
	restIndex *geoindex.ClusteringIndex
	musIndex  *geoindex.ClusteringIndex
	cinIndex  *geoindex.ClusteringIndex
}

func NewService() *Service {
	return &Service{
		restIndex: geoindex.NewClusteringIndex(),
		musIndex:  geoindex.NewClusteringIndex(),
		cinIndex:  geoindex.NewClusteringIndex(),
	}
}

func (s *Service) Load(rests []Restaurant, mus []Museum, cin []Cinema) {
	for i := range rests {
		s.restIndex.Add(&rests[i])
	}
	for i := range mus {
		s.musIndex.Add(&mus[i])
	}
	for i := range cin {
		s.cinIndex.Add(&cin[i])
	}
}

// nearby ranks attractions around a hotel whose position was already resolved by
// a targeted store lookup (mirroring the Go service, which Finds the hotel's
// lat/lon per request and then queries the pre-built geo index).
func (s *Service) nearby(index *geoindex.ClusteringIndex, lat, lon float64) []string {
	center := &geoindex.GeoPoint{Plat: lat, Plon: lon}
	pts := index.KNearest(center, maxSearchResults, geoindex.Km(maxSearchRadius), func(geoindex.Point) bool { return true })
	ids := make([]string, len(pts))
	for i, p := range pts {
		ids[i] = p.Id()
	}
	return ids
}

func (s *Service) NearbyRest(lat, lon float64) []string {
	return s.nearby(s.restIndex, lat, lon)
}

func (s *Service) NearbyMus(lat, lon float64) []string {
	return s.nearby(s.musIndex, lat, lon)
}

func (s *Service) NearbyCinema(lat, lon float64) []string {
	return s.nearby(s.cinIndex, lat, lon)
}
