#!/bin/bash
mongoimport --db review-db --collection reviews --file /seed/review-seed.json --jsonArray
