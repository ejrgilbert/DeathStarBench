package main

import (
	store "hotel-components/components/user/hotel/store/user-store"
	userapi "hotel-components/components/user/hotel/api/user"
)

var svc = NewService()

func main() {}

func init() {
	userapi.Exports.Init = doInit
	userapi.Exports.CheckUser = checkUser
}

func doInit() {
	store.Init()

	witUsers := store.LoadUsers().Slice()
	users := make([]User, len(witUsers))
	for i, u := range witUsers {
		users[i] = User{Username: u.Username, Password: u.Password}
	}
	svc.Load(users)
}

func checkUser(username string, password string) bool {
	return svc.CheckUser(username, password)
}
