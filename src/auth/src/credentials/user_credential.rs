//use std::future::Future;
use http::header::{HeaderName, HeaderValue, AUTHORIZATION};
//use serde::{Deserialize, Serialize};
use crate::credentials::Result;
//use crate::token::{Token, TokenProvider};
//use reqwest::{Client, Method, Request, Response};

//const OAUTH2_ENDPOINT: &str = "https://oauth2.googleapis.com/token";


pub(crate) struct UserCredential {
    //quota_project_id: Option<String>,
    //universe_domain: String,
    //token_provider: Box<dyn TokenProvider>,
}

#[async_trait::async_trait]
impl crate::credentials::traits::dynamic::Credential for UserCredential {  
    async fn get_headers(&mut self) -> Result<Vec<(HeaderName, HeaderValue)>> {
        let token = std::env::var("DBOLDUC_TOKEN").expect("set DBOLDUC_TOKEN env while developing. Don't want to accidentally commit secrets.");
        let mut value = HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(); // TODO : probably want to handle error
        value.set_sensitive(true);
        let headers =  vec![
            (
                AUTHORIZATION,
                value
            )];
        Ok(headers)
    }

    async fn get_universe_domain(&mut self) -> Option<String> {
        Some("googleapis.com".to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        assert!(true);
    }
}