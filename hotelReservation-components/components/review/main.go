package main

import (
	"go.bytecodealliance.org/cm"


	hkv    "hotel-components/components/review/cache/keyvalue/keyvalue"
	store  "hotel-components/components/review/hotel/store/review-store"
	revapi "hotel-components/components/review/hotel/api/review"
)

var svc = NewService()

func main() {}

func init() {
	revapi.Exports.GetReviews = getReviews
}

func getReviews(hotelId string) cm.List[revapi.ReviewComm] {
	reviews := svc.GetReviews(hotelId, fetchReviews, cacheGet, cacheSet)
	witResult := make([]revapi.ReviewComm, len(reviews))
	for i, r := range reviews {
		witResult[i] = revapi.ReviewComm{
			ReviewID:    r.ReviewId,
			HotelID:     r.HotelId,
			Name:        r.Name,
			Rating:      r.Rating,
			Description: r.Description,
			Image:       revapi.Image{URL: r.Image.Url, Default: r.Image.Default},
		}
	}
	return cm.ToList(witResult)
}

func fetchReviews(hotelId string) []Review {
	witRevs := store.GetReviews(hotelId).Slice()
	result := make([]Review, len(witRevs))
	for i, wr := range witRevs {
		result[i] = Review{
			ReviewId:    wr.ReviewID,
			HotelId:     wr.HotelID,
			Name:        wr.Name,
			Rating:      wr.Rating,
			Description: wr.Description,
			Image:       Image{Url: wr.Image.URL, Default: wr.Image.Default},
		}
	}
	return result
}

const cacheNS = "review:"

func cacheGet(key string) ([]byte, bool) {
	opt := hkv.Get(cacheNS + key)
	if opt.None() {
		return nil, false
	}
	return opt.Some().Slice(), true
}

func cacheSet(key string, val []byte) {
	hkv.Set(cacheNS+key, cm.ToList(val))
}
