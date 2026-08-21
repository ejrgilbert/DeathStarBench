// In-language-middleware baseline
package inlangmw

import (
	"os"

	"github.com/rs/zerolog/log"
	"google.golang.org/grpc"
)

func UnaryServerInterceptors() []grpc.UnaryServerInterceptor {
	switch os.Getenv("INLANG_SIGNAL") {
	case "metrics":
		log.Info().Msg("inlangmw: metrics UnaryServerInterceptor ENABLED")
		return []grpc.UnaryServerInterceptor{metricsInterceptor()}
	default:
		return nil
	}
}
