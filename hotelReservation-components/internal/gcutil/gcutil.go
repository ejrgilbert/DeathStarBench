// Package gcutil forces a garbage collection at the end of each request.
//
// Why every request (and not batched):
//
// TinyGo's GC scans the stack conservatively (runtime.markRoots →
// scanConservative). Under host-side instance reuse, each request allocates
// cabi_realloc'd cross-boundary return buffers (every host cache read, every
// component-call result) that are backed by the GC heap but only reliably
// reclaimable at a quiescent point — a shallow stack, where a conservative scan
// has no false roots. Calling Tick() at the end of handle is exactly that point.
//
// Batching (GC every N>1) is unsafe here: it lets the heap accumulate until
// TinyGo's *automatic* on-full GC (runtime.alloc, hardcoded to collect when the
// heap fills) fires in the MIDDLE of a cross-component call, while a result
// buffer is being lifted but is not yet rooted — the collector frees it and the
// guest reads freed memory (observed as "invalid utf-8" / "string pointer out
// of bounds" corruption). Running GC every request keeps the heap small enough
// that the on-full collector never triggers mid-call, so only this safe
// boundary GC ever runs.
package gcutil

import "runtime"

// Tick forces a garbage collection at a request boundary.
func Tick() {
	runtime.GC()
}
