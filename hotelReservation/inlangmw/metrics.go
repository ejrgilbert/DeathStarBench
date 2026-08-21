package inlangmw

import (
	"context"
	"sync"
	"sync/atomic"
	"time"

	"google.golang.org/grpc"
)

// metricsInterceptor is the in-language RED-metrics middleware: on each unary
// call it records per-method count, error count, and a latency-histogram sample
// into an in-memory (unscraped) registry — the interceptor's own instrumentation
// cost, with no telemetry backend.
func metricsInterceptor() grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		start := time.Now()
		resp, err := handler(ctx, req)
		observe(info.FullMethod, time.Since(start).Nanoseconds(), err != nil)
		return resp, err
	}
}

// reg maps gRPC full-method name -> *methodMetrics.
var reg sync.Map

// bounds are OTel/Prometheus-ish latency histogram bucket upper bounds, in ns
// (0.5ms … 10s). observe() finds the first bound >= the sample.
var bounds = [...]int64{
	5e5, 1e6, 2.5e6, 5e6, 1e7, 2.5e7, 5e7, 1e8, 2.5e8, 5e8, 1e9, 2.5e9, 5e9, 1e10,
}

type methodMetrics struct {
	count   uint64
	errors  uint64
	sumNs   uint64
	buckets [len(bounds) + 1]uint64 // trailing bucket = +Inf
}

func observe(method string, ns int64, isErr bool) {
	v, ok := reg.Load(method)
	if !ok {
		v, _ = reg.LoadOrStore(method, &methodMetrics{})
	}
	m := v.(*methodMetrics)
	atomic.AddUint64(&m.count, 1)
	if isErr {
		atomic.AddUint64(&m.errors, 1)
	}
	atomic.AddUint64(&m.sumNs, uint64(ns))
	i := 0
	for i < len(bounds) && ns > bounds[i] {
		i++
	}
	atomic.AddUint64(&m.buckets[i], 1)
}
