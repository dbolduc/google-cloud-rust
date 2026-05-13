Awesome! Great work.

Now I need you to implement an integration test for this service.

## Prepare the integration test driver

Put the test under
- `tests/bigquery`

Look at the `tests/bigquery/Cargo.toml` for adding `google-cloud-bigquery-write` as a dep, same as we do with `google-cloud-bigquery`.

## Write the test

Then I want a simple test that writes 10 rows to a table using the default stream.

Use some admin API to read the size of the resulting table to make sure it matches 10 rows.

The data format should be in arrow. Same with the table schema. Look at this java example for inspiration:

https://github.com/googleapis/java-bigquerystorage/blob/main/samples/snippets/src/main/java/com/example/bigquerystorage/WriteToDefaultStreamWithArrow.java

Name the test `writes` or something in `tests/bigquery/tests/driver.rs`. Call into `tests/bigquery/src/...` the way we do for the other tests in the fixture.

When you first write the test, include a `panic!("TODO : making sure the test runs")`. We want to see the tests go from failing to passing to make sure they actually work!

## Test resources

If you need to create a test resource, see if you can do it via the public clients. If not **STOP** and ask for help.

## Make the test run

To iterate, run with:

```shell
GOOGLE_CLOUD_PROJECT=dbolduc-test cargo test -p integration-tests-bigquery writes
```

Try this until the test is green.

When it passes report success!!!!
