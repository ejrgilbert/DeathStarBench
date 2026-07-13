package main

import (
	"crypto/sha256"
	"fmt"
)

type User struct {
	Username string
	Password string
}

type Service struct {
	users map[string]string
}

func NewService() *Service {
	return &Service{users: make(map[string]string)}
}

func (s *Service) Load(us []User) {
	s.users = make(map[string]string, len(us))
	for _, u := range us {
		s.users[u.Username] = u.Password
	}
}

func (s *Service) CheckUser(username, password string) bool {
	sum := sha256.Sum256([]byte(password))
	pass := fmt.Sprintf("%x", sum)
	if truePass, found := s.users[username]; found {
		return pass == truePass
	}
	return false
}
