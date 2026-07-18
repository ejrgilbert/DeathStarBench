#!/bin/bash
mongoimport --db hotel --collection geo          --file /seed/geo-seed.json                    --jsonArray
mongoimport --db hotel --collection recs         --file /seed/recommendation-seed.json          --jsonArray
mongoimport --db hotel --collection attractions  --file /seed/attractions-seed.json             --jsonArray
mongoimport --db hotel --collection users        --file /seed/user-seed.json                    --jsonArray
mongoimport --db hotel --collection rates        --file /seed/rate-seed.json                    --jsonArray
mongoimport --db hotel --collection profiles     --file /seed/profile-seed.json                 --jsonArray
mongoimport --db hotel --collection reviews      --file /seed/review-seed.json                  --jsonArray
mongoimport --db hotel --collection number       --file /seed/reservation-numbers-seed.json     --jsonArray
mongoimport --db hotel --collection reservation  --file /seed/reservation-reservations-seed.json --jsonArray
