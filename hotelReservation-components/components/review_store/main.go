package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	col      "hotel-components/components/review_store/host/storage/collection"
	revstore "hotel-components/components/review_store/hotel/store/review-store"
)

type seedImage struct {
	Url     string `json:"url"`
	Default bool   `json:"default"`
}

type seedReview struct {
	ReviewId    string    `json:"reviewId"`
	HotelId     string    `json:"hotelId"`
	Name        string    `json:"name"`
	Rating      float32   `json:"rating"`
	Description string    `json:"description"`
	Images      seedImage `json:"images"`
}

var (
	conn      col.Connection
	connOpen  bool
	reviews   []revstore.Review
	allLoaded bool
)

func main() {}

func init() {
	revstore.Exports.LoadReviews = loadReviews
}

func ensureConn() {
	if connOpen {
		return
	}
	conn = col.ConnectionOpen("reviews")
	connOpen = true
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	ensureConn()
	rawDocs := col.FindAll(conn).Slice()
	for _, raw := range rawDocs {
		var s seedReview
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		reviews = append(reviews, revstore.Review{
			ReviewID:    s.ReviewId,
			HotelID:     s.HotelId,
			Name:        s.Name,
			Rating:      s.Rating,
			Description: s.Description,
			Image: revstore.Image{
				URL:     s.Images.Url,
				Default: s.Images.Default,
			},
		})
	}
	allLoaded = true
}

func loadReviews() cm.List[revstore.Review] {
	ensureLoaded()
	return cm.ToList(reviews)
}
