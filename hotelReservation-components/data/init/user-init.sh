#!/bin/bash
mongoimport --db user-db --collection user --file /seed/user-seed.json --jsonArray
