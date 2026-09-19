// Topology capture for the equivalence harness (NOT the perf sweeps).
//
// When EQUIV_TOPOLOGY=1, every service counts the RPCs it receives, keyed by
// gRPC full method (e.g. /geo.Geo/Nearby), and periodically flushes the running
// totals to a bind-mounted file (/telemetry/rpc-<hostname>.json). The harness
// snapshots these files before/after each serially-replayed request; the delta
// is that request's service->service RPC multiset. Gated so it never touches the
// perf-measurement code paths.
package inlangmw

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"time"

	"google.golang.org/grpc"
)

const topologyDir = "/telemetry"

var (
	topoCounts sync.Map // fullMethod -> *uint64
	topoOnce   sync.Once
)

// TopologyEnabled reports whether topology capture is on for this run.
func TopologyEnabled() bool { return os.Getenv("EQUIV_TOPOLOGY") == "1" }

// topologyInterceptor counts each received RPC by full method.
func topologyInterceptor() grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		v, ok := topoCounts.Load(info.FullMethod)
		if !ok {
			v, _ = topoCounts.LoadOrStore(info.FullMethod, new(uint64))
		}
		atomic.AddUint64(v.(*uint64), 1)
		return handler(ctx, req)
	}
}

// startTopologyFlusher writes the running RPC counts to a bind-mounted file
// every 100ms. Serial replay + a quiesce longer than this interval means the
// file reflects a request's calls by the time the harness snapshots it.
func startTopologyFlusher() {
	topoOnce.Do(func() {
		_ = os.MkdirAll(topologyDir, 0o777)
		host, _ := os.Hostname()
		path := filepath.Join(topologyDir, "rpc-"+host+".json")
		go func() {
			for {
				snap := map[string]uint64{}
				topoCounts.Range(func(k, v any) bool {
					snap[k.(string)] = atomic.LoadUint64(v.(*uint64))
					return true
				})
				if b, err := json.Marshal(snap); err == nil {
					tmp := path + ".tmp"
					if os.WriteFile(tmp, b, 0o644) == nil {
						_ = os.Rename(tmp, path) // atomic replace
					}
				}
				time.Sleep(100 * time.Millisecond)
			}
		}()
	})
}
