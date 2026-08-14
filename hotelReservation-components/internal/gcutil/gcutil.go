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
//
// GC_EVERY (measurement knob): Tick() runs runtime.GC() once every GC_EVERY
// calls. Default 1 (GC every request — the safe behavior described above). Set
// the GC_EVERY env var to change it: 0 or negative disables forced GC entirely;
// N>1 batches. Any value other than 1 is for measuring GC cost only — it can
// reintroduce the heap-growth / mid-call-collection corruption the per-request
// GC prevents. The host must pass the env through (inherit_env in the wasi ctx).
package gcutil

import (
	"os"
	"runtime"
	"strconv"
)

var gcEvery = loadGCEvery()
var tickCount uint64

func loadGCEvery() int64 {
	if v, ok := os.LookupEnv("GC_EVERY"); ok {
		if n, err := strconv.ParseInt(v, 10, 64); err == nil {
			return n
		}
	}
	return 1
}

// Tick forces a garbage collection at a request boundary, subject to GC_EVERY.
func Tick() {
	if gcEvery <= 0 {
		return
	}
	tickCount++
	if tickCount%uint64(gcEvery) == 0 {
		runtime.GC()
	}
}
