#!/bin/bash
mongoimport --db recommendation-db --collection recommendation --file /seed/recommendation-seed.json --jsonArray
