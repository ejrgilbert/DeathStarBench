package main

import (

	"go.bytecodealliance.org/cm"

	hostkv "hotel-components/components/cache/host/cache/keyvalue"
	keyvalue "hotel-components/components/cache/cache/keyvalue/keyvalue"
)

// The cache component owns no state: it forwards its exported
// `cache:keyvalue/keyvalue` to the host's in-process `host:cache/keyvalue` map.
// The map must live in the host (shared across the warm instance pool), so a
// `set` on one pooled instance is visible to a `get` on another.

func main() {}

func init() {
	keyvalue.Exports.Get = func(key string) cm.Option[cm.List[uint8]] {
		opt := hostkv.Get(key)
        if opt.None() {
            return cm.None[cm.List[uint8]]()
        }
		return cm.Some(cm.ToList(opt.Some().Slice()))
	}
	keyvalue.Exports.Set = func(key string, value cm.List[uint8]) {
		hostkv.Set(key, cm.ToList(value.Slice()))
	}
	// Batched probe: one gRPC round-trip / Wasm checkout for all keys, forwarding
	// to the host map per key in-process.
	keyvalue.Exports.GetMulti = func(keys cm.List[string]) cm.List[cm.Option[cm.List[uint8]]] {
		ks := keys.Slice()
		out := make([]cm.Option[cm.List[uint8]], len(ks))
		for i, k := range ks {
			opt := hostkv.Get(k)
			if opt.None() {
				out[i] = cm.None[cm.List[uint8]]()
				continue
			}
			out[i] = cm.Some(cm.ToList(opt.Some().Slice()))
		}
		return cm.ToList(out)
	}
}
