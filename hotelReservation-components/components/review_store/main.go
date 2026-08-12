package main

import (
	"encoding/json"
	"fmt"

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
	revstore.Exports.GetReviews = getReviews
}

func toReview(s seedReview) revstore.Review {
	return revstore.Review{
		ReviewID:    s.ReviewId,
		HotelID:     s.HotelId,
		Name:        s.Name,
		Rating:      s.Rating,
		Description: s.Description,
		Image: revstore.Image{
			URL:     s.Images.Url,
			Default: s.Images.Default,
		},
	}
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
		reviews = append(reviews, toReview(s))
	}
	allLoaded = true
}

func loadReviews() cm.List[revstore.Review] {
	ensureLoaded()
	return cm.ToList(reviews)
}

// getReviews does a targeted lookup by hotel id, matching the Go review
// service's memcached-miss path `Find({"hotelId": id})`.
func getReviews(hotelID string) cm.List[revstore.Review] {
	ensureConn()
	filter := fmt.Sprintf(`{"hotelId":%q}`, hotelID)
	rawDocs := col.Find(conn, col.Document(cm.ToList([]uint8(filter)))).Slice()
	result := make([]revstore.Review, 0, len(rawDocs))
	for _, raw := range rawDocs {
		var s seedReview
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		result = append(result, toReview(s))
	}
	return cm.ToList(result)
}
