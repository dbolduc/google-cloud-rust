Hey, I need you to prototype a bigquery write client.

## Understand a sketch of the API

First, figure out the simplest sketch of the API surface.

To do this you should look up the Google Cloud BigQuery Storage Write clients for golang and java. Look at the public methods. Think about how they wrap/abstract the underlying bidirectional AppendRows stream.

Don't worry about all the possible configuration options. Just think in terms of verbs. What can the client do? How does it write data?

If the clients return an intermediate type like a StreamWriter, I want to know about it. What is the StreamWriter's interface? Don't worry about closing a stream, just focus on how does an application write data.

## Research the Rust client library API surface/internals

We want this BigQuery Write client to look and feel like other Rust clients. The most comparable are probably the pubsub subscriber and the storage (GCS) client.

Read through the client set up under `src/pubsub/src/subscriber/...` such as:
- src/pubsub/src/subscriber/client.rs
- src/pubsub/src/subscriber/client_builder.rs
- src/pubsub/src/subscriber/builder.rs
- src/pubsub/src/subscriber/transport.rs
- src/pubsub/src/subscriber/message_stream.rs

Read through the client set up under `src/storage/src/storage/...` such as:
- src/storage/src/storage/client.rs
- src/storage/src/storage/builder.rs
- src/storage/src/storage/transport.rs

## Adapt the high-level API surface to Rust crates.

For the super high-level API surface of a BQ write client, plan out the surface.

I am guessing it is something like:
- a client with a client builder
- the client has one API like: `write_stream(...) -> StreamWriter`
- the `StreamWriter` has one API like: `async fn append(...) -> Result<...>`

Write this plan to a file called: `PLAN.md`

## Implement the plan

The client should live under
- src/bigquery-write/src/client.rs

Put a client builder under:
- src/bigquery-write/src/client_builder.rs

Put a stream writer under:
- src/bigquery-write/src/stream_writer.rs

Add unit tests for all of these things that are simple! There are examples in `src/bigquery-write/src/transport.rs`

Make sure everything builds clean with:

```shell
cargo test -p google-cloud-bigquery-write
```

