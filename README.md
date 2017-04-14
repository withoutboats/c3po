# c3po - an async connection pool for tokio

C-3PO is a connection pool implementation intended for use with the tokio
ecosystem. It can manage any connection which implements a protocol based on
the `tokio_proto` traits.

C-3PO is single threaded and intended to work with a single `tokio_core`
reactor.
