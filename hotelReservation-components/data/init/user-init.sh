#!/bin/bash
mongoimport --db user-db --collection users --file /seed/user-seed.json --jsonArray
