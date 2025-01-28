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

// TODO(dbolduc) - uncomment
//#[cfg(all(test, feature = "run-integration-tests"))]
#[cfg(test)]
mod driver {
    use scoped_env::ScopedEnv;
    use auth::credentials::create_access_token_credential;
    use gax::options::ClientConfig as Config;
    use secretmanager::client::SecretManagerService;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn service_account() {
        // Default SM client
        let client = SecretManagerService::new().await.unwrap();
        
        // Load credential json
        let response = client.access_secret_version("projects/rust-auth-testing/secrets/test-sa-creds-json/versions/latest").send().await.unwrap();
        let adc_json = response.payload.unwrap().data;
        //println!("DEBUG: {:?}", adc_json);

        // Write it to a file
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.into_temp_path();
        std::fs::write(&path, adc_json).expect("Unable to write to temporary file.");
        
        // GOOGLE_APPLICATION_CREDENTIALS points to file
        let _e = ScopedEnv::set("GOOGLE_APPLICATION_CREDENTIALS", path.to_str().unwrap());
        
        // Construct new SM client with new credentials
        let creds = create_access_token_credential().await.unwrap();
        let config = Config::new().set_credential(creds);
        let client = SecretManagerService::new_with_config(config).await.unwrap();

        // Try to access the special secret
        let response = client.access_secret_version("projects/rust-auth-testing/secrets/test-sa-creds-secret/versions/latest").send().await.unwrap();
        let secret = response.payload.unwrap().data;
        assert_eq!(secret, "service_account");

        // DEBUG : fail the test to see the print statements
        //assert!(false);
    }
}