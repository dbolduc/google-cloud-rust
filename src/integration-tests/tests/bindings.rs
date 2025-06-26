// Copyright 2025 Google LLC
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

/// The relevant part of `transport.rs` for HTTP bindings
///
/// This module contains "once-generated" code that is now maintained by hand. Ideally we would use
/// our generated types here.
///
/// TODO(#2523) - replace this with generated code.
mod bindings {
    use gax::error::{Error, binding::BindingError};
    use gaxi::path_parameter::{PathMismatchBuilder, try_match2};
    use gaxi::routing_parameter::Segment;

    /// A stand in for a generated request message.
    #[derive(Default, serde::Serialize)]
    pub struct Request {
        pub name: String,
        pub project: String,
        pub location: String,
        pub id: u64,
        pub optional: Option<String>,
        pub child: Option<Box<Request>>,
    }

    /// A stand in for a generated service stub.
    pub struct TestService {
        inner: gaxi::http::ReqwestClient,
    }

    impl TestService {
        pub async fn new() -> Self {
            let config = gaxi::options::ClientConfig {
                cred: auth::credentials::testing::test_credentials().into(),
                ..Default::default()
            };
            let inner = gaxi::http::ReqwestClient::new(config, "https://test.googleapis.com")
                .await
                .expect("test credentials can never fail");
            Self { inner }
        }

        /// Once-generated code that produces a `reqwest::RequestBuilder`
        ///
        /// The code was generated off of this source proto:
        ///
        /// ```norust
        /// syntax = "proto3";
        /// package google.rust.sdk.test;
        ///
        /// import "google/api/annotations.proto";
        ///
        /// service TestService {
        ///   rpc DoFoo(Request) returns (Response) {
        ///     option (google.api.http) = {
        ///       post: "/v1/{name=projects/*/locations/*}:first"
        ///       additional_bindings {
        ///         post: "/v1/projects/{project}/locations/{location}/ids/{id}:additionalBinding"
        ///       }
        ///       additional_bindings {
        ///         get: "/v1/projects/{child.project}/locations/{child.location}/ids/{child.id}:additionalBindingOnChild"
        ///       }
        ///     };
        ///   }
        /// }
        ///
        /// message Request {
        ///     string name = 1;
        ///     string project = 2;
        ///     string location = 3;
        ///     uint64 id = 4;
        ///     optional string optional = 5;
        ///     Request child = 6;
        /// }
        ///
        /// message Response {}
        /// ```
        ///
        /// The code was copied exactly from `transport.rs`
        pub fn builder(&self, req: Request) -> gax::Result<reqwest::RequestBuilder> {
            // TODO : Darren : Regenerate this code with the final form of the template / gaxi in a
            // branch off of main.
            let builder = None
                .or_else(|| {
                    let path = format!(
                        "/v2/{}:create",
                        try_match2(
                            Some(&req).map(|m| &m.name).map(|s| s.as_str()),
                            &[
                                Segment::Literal("projects/"),
                                Segment::SingleWildcard,
                                Segment::Literal("/locations/"),
                                Segment::SingleWildcard
                            ]
                        )?,
                    );

                    let builder = (|| {
                        let builder = self.inner.builder(reqwest::Method::POST, path);
                        let builder = builder.query(&[("project", &req.project)]);
                        let builder = builder.query(&[("location", &req.location)]);
                        let builder = builder.query(&[("id", &req.id)]);
                        let builder = req
                            .optional
                            .iter()
                            .fold(builder, |builder, p| builder.query(&[("optional", p)]));
                        let builder = req
                            .child
                            .as_ref()
                            .map(|p| serde_json::to_value(p).map_err(Error::ser))
                            .transpose()?
                            .into_iter()
                            .fold(builder, |builder, v| {
                                use gaxi::query_parameter::QueryParameter;
                                v.add(builder, "child")
                            });
                        Ok(builder)
                    })();
                    Some(builder)
                })
                .or_else(|| {
                    let path = format!(
                        "/v1/projects/{}/locations/{}/ids/{}:action",
                        try_match2(
                            Some(&req).map(|m| &m.project).map(|s| s.as_str()),
                            &[Segment::SingleWildcard]
                        )?,
                        try_match2(
                            Some(&req).map(|m| &m.location).map(|s| s.as_str()),
                            &[Segment::SingleWildcard]
                        )?,
                        try_match2(Some(&req).map(|m| &m.id), &[Segment::SingleWildcard])?,
                    );

                    let builder = (|| {
                        let builder = self.inner.builder(reqwest::Method::POST, path);
                        let builder = builder.query(&[("name", &req.name)]);
                        let builder = req
                            .optional
                            .iter()
                            .fold(builder, |builder, p| builder.query(&[("optional", p)]));
                        let builder = req
                            .child
                            .as_ref()
                            .map(|p| serde_json::to_value(p).map_err(Error::ser))
                            .transpose()?
                            .into_iter()
                            .fold(builder, |builder, v| {
                                use gaxi::query_parameter::QueryParameter;
                                v.add(builder, "child")
                            });
                        Ok(builder)
                    })();
                    Some(builder)
                })
                .or_else(|| {
                    let path = format!(
                        "/v1/projects/{}/locations/{}/ids/{}:actionOnChild",
                        try_match2(
                            Some(&req)
                                .and_then(|m| m.child.as_ref())
                                .map(|m| &m.project)
                                .map(|s| s.as_str()),
                            &[Segment::SingleWildcard]
                        )?,
                        try_match2(
                            Some(&req)
                                .and_then(|m| m.child.as_ref())
                                .map(|m| &m.location)
                                .map(|s| s.as_str()),
                            &[Segment::SingleWildcard]
                        )?,
                        try_match2(
                            Some(&req).and_then(|m| m.child.as_ref()).map(|m| &m.id),
                            &[Segment::SingleWildcard]
                        )?,
                    );

                    let builder = (|| {
                        let builder = self.inner.builder(reqwest::Method::POST, path);
                        let builder = builder.query(&[("name", &req.name)]);
                        let builder = builder.query(&[("project", &req.project)]);
                        let builder = builder.query(&[("location", &req.location)]);
                        let builder = builder.query(&[("id", &req.id)]);
                        let builder = req
                            .optional
                            .iter()
                            .fold(builder, |builder, p| builder.query(&[("optional", p)]));
                        let builder = req
                            .child
                            .as_ref()
                            .map(|p| serde_json::to_value(p).map_err(Error::ser))
                            .transpose()?
                            .into_iter()
                            .fold(builder, |builder, v| {
                                use gaxi::query_parameter::QueryParameter;
                                v.add(builder, "child")
                            });
                        Ok(builder)
                    })();
                    Some(builder)
                })
                .ok_or_else(|| {
                    let mut paths = Vec::new();
                    {
                        let builder = PathMismatchBuilder::default();
                        let builder = builder.maybe_add(
                            Some(&req).map(|m| &m.name).map(|s| s.as_str()),
                            &[
                                Segment::Literal("projects/"),
                                Segment::SingleWildcard,
                                Segment::Literal("/locations/"),
                                Segment::SingleWildcard,
                            ],
                            "name",
                            "projects/*/locations/*",
                        );
                        paths.push(builder.build());
                    }
                    {
                        let builder = PathMismatchBuilder::default();
                        let builder = builder.maybe_add(
                            Some(&req).map(|m| &m.project).map(|s| s.as_str()),
                            &[Segment::SingleWildcard],
                            "project",
                            "*",
                        );
                        let builder = builder.maybe_add(
                            Some(&req).map(|m| &m.location).map(|s| s.as_str()),
                            &[Segment::SingleWildcard],
                            "location",
                            "*",
                        );
                        let builder = builder.maybe_add(
                            Some(&req).map(|m| &m.id),
                            &[Segment::SingleWildcard],
                            "id",
                            "*",
                        );
                        paths.push(builder.build());
                    }
                    {
                        let builder = PathMismatchBuilder::default();
                        let builder = builder.maybe_add(
                            Some(&req)
                                .and_then(|m| m.child.as_ref())
                                .map(|m| &m.project)
                                .map(|s| s.as_str()),
                            &[Segment::SingleWildcard],
                            "child.project",
                            "*",
                        );
                        let builder = builder.maybe_add(
                            Some(&req)
                                .and_then(|m| m.child.as_ref())
                                .map(|m| &m.location)
                                .map(|s| s.as_str()),
                            &[Segment::SingleWildcard],
                            "child.location",
                            "*",
                        );
                        let builder = builder.maybe_add(
                            Some(&req).and_then(|m| m.child.as_ref()).map(|m| &m.id),
                            &[Segment::SingleWildcard],
                            "child.id",
                            "*",
                        );
                        paths.push(builder.build());
                    }
                    gax::error::Error::binding(BindingError { paths })
                })??;

            Ok(builder)
        }
    }
}

#[cfg(test)]
mod test {
    use super::bindings::*;
    use anyhow::Result;
    use gax::error::binding::{BindingError, PathMismatch, SubstitutionFail, SubstitutionMismatch};
    use std::collections::HashSet;
    use std::error::Error as _;

    #[tokio::test]
    async fn first_match_wins() -> Result<()> {
        let stub = TestService::new().await;
        let request = Request {
            name: "projects/p/locations/l".to_string(),
            project: "ignored-p".to_string(),
            location: "ignored-l".to_string(),
            id: 12345,
            ..Default::default()
        };
        let builder = stub.builder(request)?;

        let reqwest = builder.build()?;
        let url = reqwest.url();
        assert_eq!(url.path(), "/v2/projects/p/locations/l:create");

        let actual_qps: HashSet<String> =
            url.query_pairs().map(|(key, _)| key.into_owned()).collect();
        let want_qps: HashSet<String> = ["project", "location", "id"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(actual_qps, want_qps);

        Ok(())
    }

    #[tokio::test]
    async fn additional_binding_used() -> Result<()> {
        let stub = TestService::new().await;
        let request = Request {
            name: "does-not-match-the-template".to_string(),
            project: "p".to_string(),
            location: "l".to_string(),
            id: 12345,
            ..Default::default()
        };
        let builder = stub.builder(request)?;

        let reqwest = builder.build()?;
        let url = reqwest.url();
        assert_eq!(url.path(), "/v1/projects/p/locations/l/ids/12345:action");

        // Verify we use the query parameters associated with
        // `:additionalBinding`, not `:first`
        let actual_qps: HashSet<String> =
            url.query_pairs().map(|(key, _)| key.into_owned()).collect();
        let want_qps: HashSet<String> = ["name"].iter().map(|s| s.to_string()).collect();
        assert_eq!(actual_qps, want_qps);

        Ok(())
    }

    #[tokio::test]
    async fn additional_binding_on_child() -> Result<()> {
        let stub = TestService::new().await;
        let request = Request {
            child: Some(Box::new(Request {
                project: "p".to_string(),
                location: "l".to_string(),
                id: 12345,
                ..Default::default()
            })),
            ..Default::default()
        };
        let builder = stub.builder(request)?;

        let reqwest = builder.build()?;
        let url = reqwest.url();
        assert_eq!(
            url.path(),
            "/v1/projects/p/locations/l/ids/12345:actionOnChild"
        );

        Ok(())
    }

    #[tokio::test]
    async fn no_bindings() -> Result<()> {
        let stub = TestService::new().await;
        let request = Request {
            // name: unset!!!
            project: "does/not/match/the/template".to_string(),
            location: "l".to_string(),
            // child: also unset!!!
            ..Default::default()
        };
        let e = stub
            .builder(request)
            .expect_err("Binding validation should fail");

        assert!(e.is_binding(), "{e:?}");
        assert!(e.source().is_some(), "{e:?}");
        let got = e
            .source()
            .and_then(|e| e.downcast_ref::<BindingError>())
            .expect("should be a BindingError");

        let want = BindingError {
            paths: vec![
                PathMismatch {
                    subs: vec![SubstitutionMismatch {
                        field_name: "name",
                        problem: SubstitutionFail::UnsetExpecting("*"),
                    }],
                },
                PathMismatch {
                    subs: vec![SubstitutionMismatch {
                        field_name: "project",
                        problem: SubstitutionFail::MismatchExpecting(
                            "does/not/match/the/template".to_string(),
                            "*",
                        ),
                    }],
                },
                PathMismatch {
                    subs: vec![
                        SubstitutionMismatch {
                            field_name: "child.project",
                            problem: SubstitutionFail::UnsetExpecting("*"),
                        },
                        SubstitutionMismatch {
                            field_name: "child.location",
                            problem: SubstitutionFail::UnsetExpecting("*"),
                        },
                        SubstitutionMismatch {
                            field_name: "child.id",
                            // NOTE : the gaxi code is outdated, so the expectation is incorrect.
                            //problem: SubstitutionFail::Unset,
                            problem: SubstitutionFail::UnsetExpecting("todo"),
                        },
                    ],
                },
            ],
        };
        assert_eq!(got, &want);
        Ok(())
    }
}
