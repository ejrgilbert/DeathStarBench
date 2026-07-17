package main

import (
	"encoding/json"

	"go.bytecodealliance.org/cm"

	hkv     "hotel-components/components/user/host/cache/keyvalue"
	store   "hotel-components/components/user/hotel/store/user-store"
	userapi "hotel-components/components/user/hotel/api/user"
)

func main() {}

func init() {
	userapi.Exports.CheckUser = checkUser
}

func loadUsers() []User {
	const key = "user:all"
	if opt := hkv.Get(key); !opt.None() {
		var users []User
		if err := json.Unmarshal(opt.Some().Slice(), &users); err == nil {
			return users
		}
	}
	witUsers := store.LoadUsers().Slice()
	users := make([]User, len(witUsers))
	for i, u := range witUsers {
		users[i] = User{Username: string([]byte(u.Username)), Password: string([]byte(u.Password))}
	}
	if b, err := json.Marshal(users); err == nil {
		hkv.Set(key, cm.ToList(b))
	}
	return users
}

func checkUser(username string, password string) bool {
	svc := NewService()
	svc.Load(loadUsers())
	return svc.CheckUser(username, password)
}
