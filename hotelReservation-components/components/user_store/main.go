package main

import (
	"encoding/json"
	"os"

	"go.bytecodealliance.org/cm"

	col "hotel-components/components/user_store/host/storage/collection"
	userstore "hotel-components/components/user_store/hotel/user-data/user-store"
)

type seedUser struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

var (
	users     []userstore.User
	allLoaded bool
)

func main() {}

func init() {
	userstore.Exports.Init = doInit
	userstore.Exports.LoadUsers = loadUsers
}

func doInit() {
	if col.Count() > 0 {
		return
	}

	data, err := os.ReadFile("/data/user-seed.json")
	if err != nil {
		panic("read seed file: " + err.Error())
	}

	var seeds []seedUser
	if err := json.Unmarshal(data, &seeds); err != nil {
		panic("parse seed data: " + err.Error())
	}

	docs := make([]col.Document, len(seeds))
	for i, s := range seeds {
		b, _ := json.Marshal(s)
		docs[i] = col.Document(cm.ToList(b))
	}
	col.InsertMany(cm.ToList(docs))
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	rawDocs := col.FindAll().Slice()
	for _, raw := range rawDocs {
		var s seedUser
		json.Unmarshal(cm.List[uint8](raw).Slice(), &s)
		users = append(users, userstore.User{Username: s.Username, Password: s.Password})
	}
	allLoaded = true
}

func loadUsers() cm.List[userstore.User] {
	ensureLoaded()
	return cm.ToList(users)
}
