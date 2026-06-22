## Task

We need to write an integration test to verify the Proto-based implementation

## Steps

- Need to identify the ProtoSchema and ProtoData.

I recommend this schema:

```proto
message SampleData {
  required string name = 1;
  required int64 age = 2;
}
```

And write these rows (json) to the table.

```json
{"name": "Jim", "age": 35}
{"name": "Jane", "age": 27}
```

(or feel free to reuse the same schema as the existing test, just in proto format.)

- Ideally, we define the schema in a `schema.proto` and read it. We could also construct a `DescriptorProto` by hand, but this seems brittle.

If it is too complicated, pause and I can figure this part out.

- We should add a new piece to the `writes` integration test for this new code path.

## Testing

```shell
GOOGLE_CLOUD_PROJECT=dbolduc-test cargo test -p integration-tests-bigquery --features run-integration-tests writes
```
