package inlangmw

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"time"

	"google.golang.org/grpc"
)

// spansInterceptor mints a span per call (fresh trace/span IDs + start/end
// timestamps) and drops it — generate + discard, matching the no-op sink
// (cf. Envoy tracing→dead sink, splicer otel-bare-spans host no-op).
type span struct {
	traceID string
	spanID  string
	name    string
	start   time.Time
	end     time.Time
}

func spansInterceptor() grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		s := span{traceID: randHex(16), spanID: randHex(8), name: info.FullMethod, start: time.Now()}
		resp, err := handler(ctx, req)
		s.end = time.Now()
		_ = s // dropped (generated + discarded)
		return resp, err
	}
}

func randHex(n int) string {
	b := make([]byte, n)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)
}
