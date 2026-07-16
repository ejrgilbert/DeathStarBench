package main

import (
	"go.bytecodealliance.org/cm"

	keyvalue "hotel-components/components/cache/cache/keyvalue/keyvalue"
)

var store = make(map[string][]byte)

func main() {}

func init() {
	keyvalue.Exports.Get = func(key string) cm.Option[cm.List[uint8]] {
		v, ok := store[key]
		if !ok {
			return cm.None[cm.List[uint8]]()
		}
		return cm.Some(cm.ToList(v))
	}
	keyvalue.Exports.Set = func(key string, value cm.List[uint8]) {
		k := string([]byte(key))
		b := make([]byte, len(value.Slice()))
		copy(b, value.Slice())
		store[k] = b
	}
}
