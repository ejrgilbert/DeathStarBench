#!/bin/bash

echo "[INFO] testing attractions"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto attractions.proto \
      -d '{"hotel_id":"5"}' \
      localhost:8087 attractions.Attractions/NearbyCinema; then
    echo "[ERROR] attractions failed"
    exit 1
fi


echo "[INFO] testing frontend"
# TODO

echo "[INFO] testing geo"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto geo.proto \
      -d '{"lat":"37.7854","lon":"-122.4005"}' \
      localhost:8089 geo.Geo/Nearby; then
    echo "[ERROR] geo failed"
    exit 1
fi

echo "[INFO] testing profile"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto profile.proto \
      -d '{"hotelIds":["0", "1"]}' \
      localhost:8095 profile.Profile/GetProfiles; then
    echo "[ERROR] profile failed"
    exit 1
fi

echo "[INFO] testing rate"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto rate.proto \
      -d '{"hotel_ids":["0", "1"],"in_date":"3/12/2023","out_date":"3/14/2023"}' \
      localhost:8093 rate.Rate/GetRates; then
    echo "[ERROR] rate failed"
    exit 1
fi

echo "[INFO] testing recommendation"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto recommendation.proto \
      -d '{"require":"dis","lat":37.7749,"lon":-122.4194}' \
      localhost:8085 recommendation.Recommendation/GetRecommendations; then
    echo "[ERROR] recommendation failed"
    exit 1
fi

echo "[INFO] testing reservation"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto reservation.proto \
      -d '{"customerName":"Person","hotelId":["4"],"inDate":"2015-04-09","outDate":"2015-04-10","roomNumber":1}' \
      localhost:8100 reservation.Reservation/CheckAvailability; then
    echo "[ERROR] reservation failed"
    exit 1
fi
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto reservation.proto \
      -d '{"customerName":"Elizabeth","hotelId":"1","inDate":"2015-04-09","outDate":"2015-04-10","roomNumber":200}' \
      localhost:8100 reservation.Reservation/MakeReservation; then
    echo "[ERROR] reservation failed"
    exit 1
fi
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto reservation.proto \
      -d '{"customerName":"Elizabeth","hotelId":"1","inDate":"2015-04-09","outDate":"2015-04-10","roomNumber":1}' \
      localhost:8100 reservation.Reservation/CheckAvailability; then
    echo "[ERROR] reservation failed"
    exit 1
fi
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto reservation.proto \
      -d '{"customerName":"Not me","hotelId":"2","inDate":"2015-04-09","outDate":"2015-04-10","roomNumber":201}' \
      localhost:8100 reservation.Reservation/MakeReservation; then
    echo "[ERROR] reservation failed"
    exit 1
fi

echo "[INFO] testing review"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto review.proto \
      -d '{"hotelId":"2"}' \
      localhost:8098 review.Review/GetReviews; then
    echo "[ERROR] review failed"
    exit 1
fi

echo "[INFO] testing search"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto search.proto \
      -d '{"lat":37.7749,"lon":-122.4194,"inDate":"3/12/2023","outDate":"3/14/2023"}' \
      localhost:8097 search.Search/Nearby; then
    echo "[ERROR] user failed"
    exit 1
fi

echo "[INFO] testing user (fail login)"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto user.proto \
      -d '{"username":"Cornell_30","password":"abc"}' \
      localhost:8091 user.User/CheckUser; then
    echo "[ERROR] user failed"
    exit 1
fi

echo "[INFO] testing user (pass login)"
if ! grpcurl -plaintext \
      -import-path ./proto \
      -proto user.proto \
      -d '{"username":"Cornell_30","password":"0000000000"}' \
      localhost:8091 user.User/CheckUser; then
    echo "[ERROR] user failed"
    exit 1
fi
