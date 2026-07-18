#!/bin/bash
mongoimport --db rate-db --collection rates --file /seed/rate-seed.json --jsonArray
