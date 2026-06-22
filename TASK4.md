## Background

The BQ Storage Write service accepts data and schemas in either Arrow or Proto format.

## Task

I need to prototype how to receive input in the following formats:

- ArrowSchema + ArrowData
- ProtoSchema + ProtoData

### Arrow

We already did this. Yay!

### Proto

We need to think carefully here. We should start by enumerating the tasks it will take to complete this work. Off the top of my head:

- There is no conversion for a `DescriptorProto`. (i.e. stuff under `src/bigquery-write/src/generated/convert/...`). Or rather, there is a handwritten  just make the `ProtoData`

- The internal interfaces are written in terms of `ArrowData` / `ArrowSchema`. We should probably just do a separate implementation for Proto. i.e. we keep the same client, but add a new `write_stream_proto(&self, schema: ProtoSchema) -> ProtoStreamWriter` function. It will help to read `TASK.md` and the existing implementation.

- We should only expose the `wkt::DescriptorProto` and not the `prost_types::DescriptorProto`. (We only own the `wkt` crate, but not the `prost_types` crate). You might also choose to accept a generic stream of bytes representing the serialized gRPC message for the `DescriptorProto`. That may be easier to work with and save a step of conversions.

## Testing

Build the code with:

```shell
cargo check -p google-cloud-bigquery-write
```

If it builds, then we are done. Adding integration tests is a task for the future, not for now. Please report success and await further instruction.
