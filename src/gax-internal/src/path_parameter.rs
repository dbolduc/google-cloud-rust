// Copyright 2024 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Handling of missing path parameters.
//!
//! Parameters used to build the request path (aka 'path parameters') are
//! required. But for complicated reasons they may appear in optional fields.
//! The generator needs to return an error when the parameter is missing, and
//! a small helper function makes the generated code easier to read.

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("missing required parameter {0}")]
    MissingRequiredParameter(String),
}

pub fn missing(name: &str) -> gax::error::Error {
    gax::error::Error::binding(Error::MissingRequiredParameter(name.to_string()))
}

use crate::routing_parameter::Segment;

fn matches(value: &str, template: &[Segment]) -> bool {
    crate::routing_parameter::value(Some(value), &[], template, &[]).is_some()
}

fn make_path(
    foo: String,
    baz: Option<&str>,
    number: i64,
    maybe_number: Option<i64>,
) -> Option<String> {
    if matches(
        &foo,
        &[Segment::Literal("projects/"), Segment::SingleWildcard],
    ) && matches(baz?, &[Segment::SingleWildcard])
    {
        return Some(format!(
            "v1/{}/locations/{}/id/{}/maybeId/{}:darren",
            foo, baz?, number, maybe_number?,
        ));
    }
    None
}

fn make_path2(
    foo: String,
    baz: Option<String>,
    number: i64,
    maybe_number: Option<i64>,
) -> Option<String> {
    // TODO : note that we cannot generate arg_N easily in mustache.
    // If we use the actual field names, there is the slightest of chances that we clash with `req`.... ugh, lol.
    // I can call the things: var_<name> to avoid this, lol.
    let arg1 = &foo;
    let arg2 = &baz?;
    let arg3 = &number;
    let arg4 = &maybe_number?;

    if matches(
        arg1,
        &[Segment::Literal("projects/"), Segment::SingleWildcard],
    ) && matches(arg2, &[Segment::SingleWildcard])
    {
        return Some(format!(
            "v1/{}/locations/{}/id/{}/maybeId/{}:darren",
            arg1, arg2, arg3, arg4,
        ));
    }
    None
}

fn transport() {
    // consider: "v1/{foo=projects/*}/locations/{bar.baz}/id/{number}/maybeId/{maybe_number}:darren"
    let foo: String = "projects/project".to_string();
    let baz: Option<String> = Some("location".to_string());
    let number: i64 = 12345;
    let maybe_number: Option<i64> = Some(42);

    // make path
    // TODO : the problem with this formulation is that it is not easily testable.
    let path: Option<String> = (|| {
        let arg1 = &foo;
        let arg2 = &baz?;
        let arg3 = &number;
        let arg4 = &maybe_number?;

        if matches(
            arg1,
            &[Segment::Literal("projects/"), Segment::SingleWildcard],
        ) && matches(arg2, &[Segment::SingleWildcard])
        {
            return Some(format!(
                "v1/{}/locations/{}/id/{}/maybeId/{}:darren",
                arg1, arg2, arg3, arg4,
            ));
        }
        None
    })();

    assert_eq!(
        path.as_deref(),
        Some("v1/projects/project/locations/location/id/12345/maybeId/42:darren")
    );
}

enum PathArg<'a> {
    Matching(&'a [Segment]),
    AlwaysValid,
}

#[derive(thiserror::Error, Debug)]
struct BindingError {
    paths: Vec<PathMismatch>,
}

#[derive(Debug)]
struct PathMismatch {
    subs: Vec<SubstitutionMismatch>,
}

#[derive(Debug)]
enum SubstitutionFail {
    Unset,
    UnsetExpecting(&'static str),
    // (actual, expected)
    MismatchExpecting(String, &'static str)
}

#[derive(Debug)]
struct SubstitutionMismatch {
    field_name: String,
    problem: SubstitutionFail,
}

impl std::fmt::Display for SubstitutionMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.problem {
            SubstitutionFail::Unset => {
                write!(f, "field `{}` needs to be set.", self.field_name)
            },
            SubstitutionFail::UnsetExpecting(expected) => {
                write!(f, "field `{}` needs to be set and match: '{}'", self.field_name, expected)
            },
            SubstitutionFail::MismatchExpecting(actual, expected) => {
                write!(f, "field `{}` should match: '{}'; found: '{}'", self.field_name, expected, actual)
            },
        }
    }
}

impl std::fmt::Display for PathMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, sub) in self.subs.iter().enumerate() {
            if i != 0 {
                write!(f, " AND ")?;
            }
            write!(f, "{}", sub)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for BindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the RPC cannot be sent, at least one of the conditions must be met: ")?;
        for (i, sub) in self.paths.iter().enumerate() {
            if i != 0 {
                write!(f, " OR ")?;
            }
            write!(f, "({}) {}", i + 1, sub)?;
        }
        Ok(())
    }
}

// To generate, I'd like:
// - List of { arg_name, accessor, leafTypez == string, optional, matcher template}
// - path format string

// plus whatever we need for the error messages.

fn transport_more_generaler() -> Result<(), BindingError> {
    // consider: "v1/{foo=projects/*}/locations/{bar.baz}/id/{number}/maybeId/{maybe_number}:darren"
    let foo: String = "projects/project".to_string();
    let baz: Option<String> = Some("location".to_string());
    let number: i64 = 12345;
    let maybe_number: Option<i32> = Some(42);

    // make path
    // TODO : the problem with this formulation is that it is not easily testable.
    //      : I am going to rely on GAPIC showcase for testing. They return the path bindings.
    //let path: Option<String> = None
    let path = None
    .or_else(|| {
        let arg1 = &foo;
        let arg2 = baz.as_ref()?;
        let arg3 = &number;
        let arg4 = &maybe_number?;

        // Can skip the `if` if matches returns an Option<> and we ? it.
        if !matches(
            arg1,
            &[
                Segment::Literal("projects/"),
                Segment::SingleWildcard,
            ]
        ) { return None; }
        if !matches(arg2, &[
                Segment::SingleWildcard,
            ]
        ) { return None; }

        Some(format!(
            "v1/{}/locations/{}/id/{}/maybeId/{}:darren",
            arg1,
            arg2,
            arg3,
            arg4,
        ))
    })
    .or_else(|| {
        // will look the same as above
        Some("backup path".to_string())
    })
    .ok_or_else(|| {
        let mut paths = Vec::new();
        {
            let mut subs = Vec::new();
            {
                let arg = &foo;
                if !matches(
                    arg,
                    &[
                        Segment::Literal("projects/"),
                        Segment::SingleWildcard,
                    ]
                ) {
                    subs.push(SubstitutionMismatch {
                        field_name: "foo".to_string(),
                        problem: SubstitutionFail::MismatchExpecting(foo.clone(), "projects/*"),
                    })
                }
            }
            {
                match baz.as_ref() {
                    None => {
                        subs.push(SubstitutionMismatch {
                            field_name: "bar.baz".to_string(),
                            problem: SubstitutionFail::UnsetExpecting("projects/*")
                        });
                    },
                    Some(arg) if !matches(arg, &[Segment::SingleWildcard]) => {
                        subs.push(SubstitutionMismatch {
                            field_name: "bar.baz".to_string(),
                            problem: SubstitutionFail::MismatchExpecting(arg.clone(), "projects/*")
                        });
                    },
                    _ => {}
                }
            }
            {
                if maybe_number.is_none() {
                    subs.push(SubstitutionMismatch {
                        field_name: "bar.baz".to_string(),
                        problem: SubstitutionFail::Unset
                    });
                }
            }
            paths.push(PathMismatch { subs });
        }
        BindingError { paths }
    })?;

    assert_eq!(
        path,
        "v1/projects/project/locations/location/id/12345/maybeId/42:darren"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Error;
    use std::error::Error as _;

    #[test]
    fn missing() {
        let e = super::missing("abc123");
        let fmt = format!("{e}");
        assert!(fmt.contains("abc123"), "{e:?}");
        let source = e.source().and_then(|e| e.downcast_ref::<Error>());
        assert!(
            matches!(source, Some(Error::MissingRequiredParameter(p)) if p == "abc123"),
            "{e:?}"
        );
    }

    #[test]
    fn darren() -> anyhow::Result<()> {
        super::transport();
        super::transport_more_generaler()?;
        Ok(())
    }
}
