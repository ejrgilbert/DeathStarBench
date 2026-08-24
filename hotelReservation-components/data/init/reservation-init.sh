#!/bin/bash
mongoimport --db reservation-db --collection number      --file /seed/reservation-numbers-seed.json      --jsonArray
mongoimport --db reservation-db --collection reservation --file /seed/reservation-reservations-seed.json --jsonArray

# Index the fields the reservation query filters on
mongosh --quiet reservation-db --eval '
  db.reservation.createIndex({ hotelId: 1, inDate: 1, outDate: 1 });
  db.number.createIndex({ hotelId: 1 });
'
