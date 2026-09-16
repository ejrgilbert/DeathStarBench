#!/bin/bash
mongoimport --db hotel --collection geo          --file /seed/geo-seed.json                    --jsonArray
mongoimport --db hotel --collection recommendation         --file /seed/recommendation-seed.json          --jsonArray
mongoimport --db hotel --collection attractions  --file /seed/attractions-seed.json             --jsonArray
mongoimport --db hotel --collection user        --file /seed/user-seed.json                    --jsonArray
mongoimport --db hotel --collection inventory        --file /seed/rate-seed.json                    --jsonArray
mongoimport --db hotel --collection hotels     --file /seed/profile-seed.json                 --jsonArray
mongoimport --db hotel --collection reviews      --file /seed/review-seed.json                  --jsonArray
mongoimport --db hotel --collection number       --file /seed/reservation-numbers-seed.json     --jsonArray
mongoimport --db hotel --collection reservation  --file /seed/reservation-reservations-seed.json --jsonArray
