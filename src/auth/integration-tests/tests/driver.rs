// TODO(dbolduc) - uncomment
//#[cfg(all(test, feature = "run-integration-tests"))]
#[cfg(test)]
mod driver {
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn service_account() {
        // Default SM client
        let client = secretmanager::client::SecretManagerService::new().await.unwrap();
        
        // Load credential json
        let response = client.access_secret_version("projects/rust-auth-testing/secrets/test-sa-creds-secret/versions/latest").send().await.unwrap();
        let adc_json = response.payload.unwrap().data;
        println!("DEBUG: {:?}", adc_json);

        // Write it to a file

        // GOOGLE_APPLICATION_CREDENTIALS points to file
        // TODO : serial_test

        // Construct credentials

        // 
    }
}