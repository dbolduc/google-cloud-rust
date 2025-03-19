## Background

https://doc.rust-lang.org/cargo/reference/unstable.html#public-dependency

## Changes

I made some manual changes to the crates upstream of `google-cloud-gax` (at the
moment).

Dependencies are assumed to be private, so we add `public = true` to any
dependency that is...
- an external crate with a stable version. (these are safe to expose).
- one of our non-internal crates. (these will eventually be 1.0, and are for
  public consumption).

## Building

We build with:

```sh
RUSTFLAGS="-D exported-private-dependencies" \
  cargo +nightly build -Zpublic-dependency -p google-cloud-gax
```

A normal build prints unseemly warnings:

```sh
cargo build -p google-cloud-gax
```

There is one of these warnings for each line that uses `public` in each
`Cargo.toml`. I do not know how to suppress them.

```
warning: /home/dbolduc/code/git/google-cloud-rust/src/generated/rpc/types/Cargo.toml: ignoring `public` on dependency bytes, pass `-Zpublic-dependency` to enable support for it
```

## Findings:

This thing only caught false positives. Yay?

It flagged `auth` types that are not exposed outside of the crate. Hence the
changes to `jws.rs`.
