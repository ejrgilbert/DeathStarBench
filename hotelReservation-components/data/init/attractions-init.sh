#!/bin/bash
mongoimport --db attractions-db --collection attractions --file /seed/attractions-seed.json --jsonArray
