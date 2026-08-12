package main

import (
	store   "hotel-components/components/user/hotel/store/user-store"
	userapi "hotel-components/components/user/hotel/api/user"
)

func main() {}

func init() {
	userapi.Exports.CheckUser = checkUser
}

// svc is loaded once (on the first request) and reused across all subsequent
// requests on this instance — matching the Go original, which loads the users
// collection once in Server.Run() and then does in-memory lookups per request.
// This requires host-side instance reuse; with fresh-instance-per-request it
// would reload the full collection every time.
var svc *Service

func loadUsers() []User {
	witUsers := store.LoadUsers().Slice()
	users := make([]User, len(witUsers))
	for i, u := range witUsers {
		users[i] = User{Username: string([]byte(u.Username)), Password: string([]byte(u.Password))}
	}
	return users
}

func checkUser(username string, password string) bool {
	if svc == nil {
		svc = NewService()
		svc.Load(loadUsers())
	}
	return svc.CheckUser(username, password)
}
