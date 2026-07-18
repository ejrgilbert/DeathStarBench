#!/bin/bash
mongoimport --db reservation-db --collection number      --file /seed/reservation-numbers-seed.json      --jsonArray
mongoimport --db reservation-db --collection reservation --file /seed/reservation-reservations-seed.json --jsonArray
