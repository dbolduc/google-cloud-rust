Hey, I want to prototype multiplexing in `src/bigquery-write`.

Can you read @SPEC.md and tell me your initial thoughts on the design? Does it make sense? Will it work? Is it performant? Is there anything else you would suggest?

Please read the full @CHAT.md to understand why the design is the way that it is.


When you have done this, if you agree with it, please come up with a plan to implement it in `src/bigquery-write`

I know the `Write` client will hold the `Arc<Pool>` and share it with each `DefaultWriter` it creates.
- `src/bigquery-write/src/client.rs`

I think the `Pool` belongs in its own file:
- `src/bigquery-write/src/pool.rs`

The `WriterHandle` equivalent is going to be the `AppendBuilder::send()`

For now, we can implement this assuming multiplexing is always enabled.

Also, don't bother with retries. I think it will make things too complicated at the moment.

If you implement the watchdog task, put it in its own file: 
- `src/bigquery-write/src/watchdog.rs`

Please constantly build and test the code with:
```
cargo test -p google-cloud-bigquery-write
```
