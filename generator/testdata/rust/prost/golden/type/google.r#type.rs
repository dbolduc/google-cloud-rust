// This file is @generated by prost-build.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Expr {
    #[prost(string, tag = "1")]
    pub expression: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub title: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub description: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub location: ::prost::alloc::string::String,
}
impl ::prost::Name for Expr {
    const NAME: &'static str = "Expr";
    const PACKAGE: &'static str = "google.type";
    fn full_name() -> ::prost::alloc::string::String {
        "google.type.Expr".into()
    }
    fn type_url() -> ::prost::alloc::string::String {
        "type.googleapis.com/google.type.Expr".into()
    }
}
