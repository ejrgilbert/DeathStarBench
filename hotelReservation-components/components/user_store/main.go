package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	col       "hotel-components/components/user_store/host/storage/collection"
	userstore "hotel-components/components/user_store/hotel/store/user-store"
)

type seedUser struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

var (
	conn      col.Connection
	connOpen  bool
	users     []userstore.User
	allLoaded bool
)

func main() {}

func init() {
	userstore.Exports.LoadUsers = loadUsers
}

func ensureConn() {
	if connOpen {
		return
	}
	conn = col.ConnectionOpen("users")
	connOpen = true
}

func ensureLoaded() {
	if allLoaded {
		return
	}
	ensureConn()
	rawDocs := col.FindAll(conn).Slice()
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
