#!/bin/bash
mongoimport --db rate-db --collection inventory --file /seed/rate-seed.json --jsonArray
