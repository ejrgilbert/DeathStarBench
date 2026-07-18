#!/bin/bash
mongoimport --db profile-db --collection profiles --file /seed/profile-seed.json --jsonArray
