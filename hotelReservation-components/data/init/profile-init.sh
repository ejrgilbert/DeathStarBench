#!/bin/bash
mongoimport --db profile-db --collection hotels --file /seed/profile-seed.json --jsonArray
