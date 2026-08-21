package inlangmw

import (
	"context"
	"io"
	"time"

	"github.com/rs/zerolog"
	"google.golang.org/grpc"
	"google.golang.org/grpc/status"
)

// logsInterceptor emits one structured log record per call (method, code,
// duration) to a discard writer — format + drop, matching the no-op sink
// (cf. Envoy access_log→/dev/null, splicer otel-bare-logs host no-op).
var logSink = zerolog.New(io.Discard).With().Timestamp().Logger()

func logsInterceptor() grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		start := time.Now()
		resp, err := handler(ctx, req)
		logSink.Info().
			Str("method", info.FullMethod).
			Str("code", status.Code(err).String()).
			Dur("duration", time.Since(start)).
			Msg("grpc call")
		return resp, err
	}
}
