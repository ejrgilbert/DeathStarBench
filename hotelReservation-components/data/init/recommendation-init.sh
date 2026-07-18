#!/bin/bash
mongoimport --db recommendation-db --collection recs --file /seed/recommendation-seed.json --jsonArray
