Hey, I need to prototype how to receive input in the following formats:

- ArrowSchema + ArrowData
- ProtoSchema + ProtoData

## Arrow

The prototype is currently using `Arrow`. We can see that in the integration test under `tests/bigquery/src/lib.rs` `writes()`.


The thing is, this integration test looks brittle as all fuck. What is up with the `schema_buf` and manually writing the end-of-stream as bytes? That seems like something that should not be done manually. I am suspicious of it.

Can you identify the right way to do this? e.g. find an appropriate library that can handle that aspect of things and refactor the code? Or maybe it is not needed for some reason?

Also, managing the footer seems brittle too. It is a major code smell. Fix that too. Please use the `arrow-rs` library if it is available!

Recall that you can run the integration test with:

```shell
GOOGLE_CLOUD_PROJECT=dbolduc-test cargo test -p integration-tests-bigquery writes
```

When you clean up the Arrow implementation, pause and report your results.
