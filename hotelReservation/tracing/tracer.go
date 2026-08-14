package tracing

import (
	opentracing "github.com/opentracing/opentracing-go"
)

// Init returns a no-op tracer.
//
// Tracing is disabled for the benchmark so the native baseline does not pay
// span-creation/reporting overhead that the wasm deployments don't have. (Even
// the previous 1%-probabilistic sampler created spans per request.) With the
// no-op tracer, StartSpanFromContext returns zero-cost spans.
func Init(serviceName, host string) (opentracing.Tracer, error) {
	return opentracing.NoopTracer{}, nil
}
