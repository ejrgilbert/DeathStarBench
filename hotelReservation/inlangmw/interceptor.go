// In-language-middleware baseline
package inlangmw

import (
	"os"

	"github.com/rs/zerolog/log"
	"google.golang.org/grpc"
)

func UnaryServerInterceptors() []grpc.UnaryServerInterceptor {
	var interceptors []grpc.UnaryServerInterceptor

	// Equivalence-harness topology capture (gated by EQUIV_TOPOLOGY; independent
	// of the perf instrumentation below). Prepended so it counts every RPC.
	if TopologyEnabled() {
		log.Info().Msg("inlangmw: EQUIV_TOPOLOGY RPC capture ENABLED")
		startTopologyFlusher()
		interceptors = append(interceptors, topologyInterceptor())
	}

	switch os.Getenv("INLANG_BUILTIN") {
	case "otel-bare-metrics":
		log.Info().Msg("inlangmw: otel-bare-metrics interceptor ENABLED")
		interceptors = append(interceptors, metricsInterceptor())
	case "otel-bare-logs":
		log.Info().Msg("inlangmw: otel-bare-logs interceptor ENABLED")
		interceptors = append(interceptors, logsInterceptor())
	case "otel-bare-spans":
		log.Info().Msg("inlangmw: otel-bare-spans interceptor ENABLED")
		interceptors = append(interceptors, spansInterceptor())
	}
	return interceptors
}
