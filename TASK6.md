## Background

Hey, read the proposed internals for the BigQuery Write client here:

https://docs.google.com/document/d/13oVZfDkx3J7skgGa7xSNH3DaxH0bVrDBH9ISNJMr-1s/edit?resourcekey=0-6Qe3nSB4ba-BMhjsBZUXYg&tab=t.0

If you cannot read the document, PANIC! Stop and report to me, and I will help.

Then check the current implementation progress with:

```shell
git diff upstream/main
```

Note that the `runner.rs` was an early prototype, similar to a "stream task" from the design document.

## Task

We want to start to move the implementation over to more match the design document. That probably looks like fleshing out the `pool.rs` type and using it to replace `runner.rs`. (or you start from runner.rs and make it look more like the design. You need to figure out the plan.)

We do not need to implement the full spec. We don't need to implement any sort of retries, or even really multiplexing. (So no need for retries opening a gRPC stream, or on receiving a retryable response, or on a write.)

The main thing we want to accomplish is using the composable interface so that we can introduce multiplexing and retries, while keeping all upper layers the same. As written, it is similar to the `RequestPair` type.

You should come up with a plan to begin this implementation. DO NOT START THE IMPLEMENTATION, we should have a solid plan before starting this. Work with me on the plan.

I think it will look like some internal trait (but maybe not. Keep your mind open. Consider a few things and recommend the one that is best). It should be similar in signature to the `append()` function on the stream writers. Basically, something given an `AppendRowsRequest` and returning a `Result<AppendRowsResponse>`. (Or something that takes a `RequestPair` and writes to the oneshot::Sender channel)

## Testing

```shell
GOOGLE_CLOUD_PROJECT=dbolduc-test cargo test -p integration-tests-bigquery --features run-integration-tests writes
```
