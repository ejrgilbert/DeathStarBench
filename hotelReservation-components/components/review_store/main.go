package main

import (
	"encoding/json"
	"os"

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
	reviews   []revstore.Review
	allLoaded bool
)

func main() {}

func init() {
	revstore.Exports.Init        = doInit
	revstore.Exports.LoadReviews = loadReviews
}

func doInit() {
	if col.Count() > 0 {
		return
	}
	data, err := os.ReadFile("/data/review-seed.json")
	if err != nil {
		panic("read seed file: " + err.Error())
	}
	var seeds []seedReview
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
