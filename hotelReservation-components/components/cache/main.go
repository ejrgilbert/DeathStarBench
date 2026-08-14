package main

import (
	gcutil "hotel-components/internal/gcutil"

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
		opt := hostkv.Get(string([]byte(key)))
		if opt.None() {
			return cm.None[cm.List[uint8]]()
		}
		// Copy the bytes out of the host result buffer before re-lowering: the
		// caller reads the returned list after this export returns, and TinyGo's
		// GC can reclaim the un-copied cabi_realloc buffer (see internal/gcutil).
		src := opt.Some().Slice()
		b := make([]byte, len(src))
		copy(b, src)
		gcutil.Tick()
		return cm.Some(cm.ToList(b))
	}
	keyvalue.Exports.Set = func(key string, value cm.List[uint8]) {
		k := string([]byte(key))
		b := make([]byte, len(value.Slice()))
		copy(b, value.Slice())
		hostkv.Set(k, cm.ToList(b))
	}
	// Batched probe: one gRPC round-trip / Wasm checkout for all keys, forwarding
	// to the host map per key in-process.
	keyvalue.Exports.GetMulti = func(keys cm.List[string]) cm.List[cm.Option[cm.List[uint8]]] {
		ks := keys.Slice()
		out := make([]cm.Option[cm.List[uint8]], len(ks))
		for i, k := range ks {
			opt := hostkv.Get(string([]byte(k)))
			if opt.None() {
				out[i] = cm.None[cm.List[uint8]]()
				continue
			}
			src := opt.Some().Slice()
			b := make([]byte, len(src))
			copy(b, src)
			out[i] = cm.Some(cm.ToList(b))
		}
		gcutil.Tick()
		return cm.ToList(out)
	}
}
