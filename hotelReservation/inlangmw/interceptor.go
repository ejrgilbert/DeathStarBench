// In-language-middleware baseline
package inlangmw

import (
	"os"

	"github.com/rs/zerolog/log"
	"google.golang.org/grpc"
)

func UnaryServerInterceptors() []grpc.UnaryServerInterceptor {
	switch os.Getenv("INLANG_BUILTIN") {
	case "otel-bare-metrics":
		log.Info().Msg("inlangmw: otel-bare-metrics interceptor ENABLED")
		return []grpc.UnaryServerInterceptor{metricsInterceptor()}
	case "otel-bare-logs":
		log.Info().Msg("inlangmw: otel-bare-logs interceptor ENABLED")
		return []grpc.UnaryServerInterceptor{logsInterceptor()}
	case "otel-bare-spans":
		log.Info().Msg("inlangmw: otel-bare-spans interceptor ENABLED")
		return []grpc.UnaryServerInterceptor{spansInterceptor()}
	default:
		return nil
	}
}
