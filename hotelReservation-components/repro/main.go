package main

import (
	"fmt"
	"runtime"
	"strings"

	"go.bytecodealliance.org/cm"
	checker "repro/repro/gc-bug/checker"
	source "repro/repro/gc-bug/source"
)

func main() {}

func init() {
	checker.Exports.Check = check
	checker.Exports.CheckFixed = checkFixed
}

func expectedID(i int) string {
	base := fmt.Sprintf("item-%04d", i)
	return base + strings.Repeat("0", 500-len(base))
}

// check — shows the bug.
//
// cabi_realloc allocates with typptr=0 (no GC type metadata).  When the
// canonical ABI adapter builds a list<item>, it allocates the outer item array
// first, then allocates each item's tag sub-array and string-byte buffers.
// All of these are held in WASM locals — not on the Go shadow stack — so the GC
// can't see them.  With enough allocation pressure, the GC fires mid-adapter,
// frees the outer array, and subsequent cabi_realloc calls reuse its memory.
// The returned list is silently corrupted: string headers now point into
// whatever cabi_realloc wrote there next.
//
// We detect corruption by checking it.ID's length: a garbage len field produces
// a mismatch against 500 without dereferencing the invalid ptr.
func check(n, tagsPer uint32) cm.List[bool] {
	witItems := source.GetItems(n, tagsPer)
	s := witItems.Slice()
	results := make([]bool, len(s))
	for i, it := range s {
		results[i] = it.ID == expectedID(i)
	}
	return cm.ToList(results)
}

// checkFixed — shows the fix: string([]byte(s)) at the WIT boundary.
//
// Even when the outer array survives intact, its string-byte buffers (B_i) are
// still doomed: they are only reachable through the outer array, which has
// typptr=0, so the GC cannot trace into it.  Any subsequent Go allocation can
// trigger a collection that frees B_i, leaving it.ID.ptr dangling.
// Copying each string onto the Go-managed heap immediately — before any further
// allocation — moves the bytes somewhere the GC can trace, so they survive.
//
// The 2 MB sentinel raises the threshold so GetItems completes without
// mid-adapter GC (matching the warmed-up heap state of a production service).
// Dropping it before runtime.GC() then proves the copies survive the next cycle.
func checkFixed(n, tagsPer uint32) cm.List[bool] {
	ids := func() []string {
		sentinel := make([]byte, 2*1024*1024) // keep GC threshold above GetItems cost
		_ = sentinel
		witItems := source.GetItems(n, tagsPer)
		s := witItems.Slice()
		out := make([]string, len(s))
		for i, it := range s {
			out[i] = string([]byte(it.ID)) // copy to Go heap before any GC can run
		}
		return out
	}() // sentinel and witItems leave the shadow stack here; B_i is now unreachable
	runtime.GC() // confirm the Go copies in ids[] survive without the WIT buffers
	results := make([]bool, len(ids))
	for i, id := range ids {
		results[i] = id == expectedID(i)
	}
	return cm.ToList(results)
}
