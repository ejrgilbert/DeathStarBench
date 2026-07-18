#!/bin/bash
mongoimport --db geo-db --collection geo --file /seed/geo-seed.json --jsonArray
