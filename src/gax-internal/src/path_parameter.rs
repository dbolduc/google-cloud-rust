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

#[cfg(test)]
mod tests {
    use super::Error;
    use std::{error::Error as _, ops::Sub};

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
    
    use crate::routing_parameter::*;
    #[test]
    fn empty_is_not_a_match() {
        let path = value(
                    Some(""),
                    &[],
                    &[Segment::SingleWildcard],
                    &[],
        );
        assert!(path.is_none());
    }

    // consider: "/v1/{parent=projects/*/locations/*}/clusters"
    use test_case::test_case;
    #[test_case("projects/p/locations/l", Some("/v1/projects/p/locations/l/clusters"))]
    #[test_case("", None)]
    #[test_case("projects/p", None)]
    #[test_case("projects//locations/l", None)]
    #[test_case("projects/p/locations/", None)]
    #[test_case("projects/p1/p2/locations/l", None)]
    #[test_case("projects/p1/locations/l1/l2", None)]
    fn matching_variable(parent: &str, expected_path: Option<&str>) {
        let path = value(
                    Some(parent),
                    &[],
                    &[
                        Segment::Literal("projects/"),
                        Segment::SingleWildcard,
                        Segment::Literal("/locations/"),
                        Segment::SingleWildcard,
                    ],
                    &[],
        // The problem with map is that I can't compose it when we have multiple variables
        ).map(|parent| format!("/v1/{parent}/clusters"));
        assert_eq!(path.as_deref(), expected_path);
    }
    
    // consider: "/v1/projects/{project_id}/zones/{zone}/clusters"
    #[test_case("p", "z", Some("/v1/projects/p/zones/z/clusters"))]
    #[test_case("p", "", None)]
    #[test_case("", "z", None)]
    #[test_case("p1/p2", "z", None)]
    #[test_case("p", "z1/z2", None)]
    #[test_case("p/", "z", None)]
    #[test_case("p", "z/", None; "deconflict name")]
    fn matching_multiple(project_id: &str, zone: &str, expected_path: Option<&str>) {
        let m1 = value(
                    Some(project_id),
                    &[],
                    &[Segment::SingleWildcard],
                    &[],
        );
        let m2 = value(
                    Some(zone),
                    &[],
                    &[Segment::SingleWildcard],
                    &[],
        );
        let mut path = None;
        if let Some(m1) = m1 {
            if let Some(m2) = m2 {
                path = Some(format!("/v1/projects/{m1}/zones/{m2}/clusters"))
            }
        }
        assert_eq!(path.as_deref(), expected_path);
    }

    #[test]
    fn match_chaining() -> anyhow::Result<()> {
        let none = None;
        let some = Some("path");
        let ignored = Some("ignored");

        let path =
                none
                .or_else(|| none)
                .or_else(|| some)
                .or_else(|| ignored)
            .ok_or_else(|| super::missing("temp"))?;
        assert_eq!(path, "path");
        Ok(())
    }

    // Thoughts, we can modify `value` to return true or false for us. We only ever paste in the thing.
    // meh, that doesn't save us any trouble tbh.
    // 
    // We could build the path in-line, and if we ever hit something we can't sub, we move on to the next path
    // Pro: most APIs do not have additional bindings. This is optimal for them.
    //
    // I don't think it helps us much to cache the substitutions. e.g. we could avoid validating a field multiple times "projects/*/locations/*/clusters/*"

    // compute substitutions, if any error, try next path. If no next path, binding error.
    enum PathSegment<'a> {
        Literal(&'static str),
        Variable(&'a str, &'static[Segment]),
    }

    impl<'a> PathSegment<'a> {
        fn as_str(&self) -> &'a str {
            match self {
                PathSegment::Literal(s) => s,
                PathSegment::Variable(s, _) => s,
            }
        }
    }

    fn check_path<'a>(template: &[PathSegment]) -> bool {
        for segment in template {
            if let PathSegment::Variable(v, matcher) = segment {
                if value(Some(v), &[], matcher, &[]).is_none() {
                    return false;
                }
            }
        }
        true
    }

    fn make_path(template: &[PathSegment]) -> Option<String> {
        use std::fmt::Write;
        if !check_path(template) {
            return None;
        }
        let mut path = String::new();
        for segment in template {
            write!(&mut path, "{}", segment.as_str()).ok()?;
        }
        Some(path)
    }

    // considering: "/v1/{parent=projects/*/locations/*}/clusters"
    #[test_case("projects/p/locations/l", Some("/v1/projects/p/locations/l/clusters"))]
    #[test_case("", None)]
    #[test_case("projects/p", None)]
    #[test_case("projects//locations/l", None)]
    #[test_case("projects/p/locations/", None)]
    #[test_case("projects/p1/p2/locations/l", None)]
    #[test_case("projects/p1/locations/l1/l2", None)]
    fn matching_variable_try2(parent: &str, expected_path: Option<&str>) {
        let template = &[
            PathSegment::Literal("/v1/"),
            PathSegment::Variable(parent, &[
                    Segment::Literal("projects/"),
                    Segment::SingleWildcard,
                    Segment::Literal("/locations/"),
                    Segment::SingleWildcard,
                ]
            ),
            PathSegment::Literal("/clusters"),
        ];

        let path = make_path(template);
        assert_eq!(path.as_deref(), expected_path);
    }

// consider: "/v1/projects/{project_id}/zones/{zone}/clusters"
    #[test_case("p", "z", Some("/v1/projects/p/zones/z/clusters"))]
    #[test_case("p", "", None)]
    #[test_case("", "z", None)]
    #[test_case("p1/p2", "z", None)]
    #[test_case("p", "z1/z2", None)]
    #[test_case("p/", "z", None)]
    #[test_case("p", "z/", None; "deconflict name")]
    fn matching_multiple_try2(project_id: &str, zone: &str, expected_path: Option<&str>) {
        let template = &[
            PathSegment::Literal("/v1/projects/"),
            PathSegment::Variable(project_id, &[
                    Segment::SingleWildcard,
                ]
            ),
            PathSegment::Literal("/zones/"),
            PathSegment::Variable(zone, &[
                    Segment::SingleWildcard,
                ]
            ),
            PathSegment::Literal("/clusters"),
        ];

        let path = make_path(template);
        assert_eq!(path.as_deref(), expected_path);
    }

    #[test_case("projects/p/locations/l", "p", "z", Some("/v1/projects/p/locations/l/clusters"); "both match, first wins")]
    #[test_case("projects/p/locations/l", "", "", Some("/v1/projects/p/locations/l/clusters"); "first only")]
    #[test_case("", "p", "z", Some("/v1/projects/p/zones/z/clusters"); "second only")]
    #[test_case("", "", "", None; "neither")]
    fn match_chaining_general(parent: &str, project_id: &str, zone: &str, expected_path: Option<&str>) -> anyhow::Result<()> {
        let path =
        make_path(&[
            PathSegment::Literal("/v1/"),
            PathSegment::Variable(parent, &[
                    Segment::Literal("projects/"),
                    Segment::SingleWildcard,
                    Segment::Literal("/locations/"),
                    Segment::SingleWildcard,
                ]
            ),
            PathSegment::Literal("/clusters"),
        ])
        .or_else(||
        make_path(&[
            PathSegment::Literal("/v1/projects/"),
            PathSegment::Variable(project_id, &[
                    Segment::SingleWildcard,
                ]
            ),
            PathSegment::Literal("/zones/"),
            PathSegment::Variable(zone, &[
                    Segment::SingleWildcard,
                ]
            ),
            PathSegment::Literal("/clusters"),
        ])
        )
        ;//.ok_or_else(|| super::missing("temp"))?;
        assert_eq!(path.as_deref(), expected_path);
        Ok(())
    }

    // TODO : what is the ideal form of a binding error?
    // We can say like:
    // - field `foo` did not match: <template>
    // - AND field `bar` did not match: <template>
    // - etc.
    //
    // Or: "Could not write path = `/v1/{project=projects/*}/locations`; `project` field was not set."
    // Or: "Could not write path = `/v1/{project=projects/*}/locations`; project did not match."
    //
    // If so, I think we would want to report exactly what fails in check_path, instead of a true/false??
    //
    // Alternatively we just have a canned error that is not specific to the request. Maybe the path has {foo}, {bar}, {baz} and we just say check foo, bar, and baz.
    //
    // The far and away most common case is a missing parameter. It would be nice to surface which one that is.
    //
    // This could use a quick one-pager.

    // On errors....
    // 
    // Cases: in general it is just that the parameter is in the wrong format. That can arise from it being empty or in the wrong format.
    
    #[derive(thiserror::Error, Debug)]
    struct BindingError {
        paths: Vec<Vec<SubstitutionMismatch>>,
    }

    #[derive(Debug)]
    struct SubstitutionMismatch {
        field: String,
        expected: String,
        actual: String,
    }

    impl std::fmt::Display for SubstitutionMismatch {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "field `{}` does not match: '{}'; found:'{}'", self.field, self.expected, self.actual)
        }
    }

    impl std::fmt::Display for BindingError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for path in &self.paths {
                write!(f, "Could not match path:")?;
                for sub in path {
                    write!(f, "{:?}", sub)?;
                }
            }
            Ok(())
        }
    }

    fn check_path_full<'a>(template: &[PathSegment]) -> Result<(), BindingError> {
        let paths = Vec::new();
        for segment in template {
            let path = Vec::new();
            if let PathSegment::Variable(v, matcher) = segment {
                if value(Some(v), &[], matcher, &[]).is_none() {
                    path.push(SubstitutionMismatch {
                        field: "TODO".to_string(),
                        expected: matcher.to_string()

                    });
                }
            }
        }
        true
    }
}
