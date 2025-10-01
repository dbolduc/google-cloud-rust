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

#[cfg(test)]
mod tests {
    use aiplatform_grpc::Client;
    use aiplatform_grpc::google::cloud::aiplatform::v1;
    use gax::options::RequestOptions;

    #[tokio::test]
    async fn quickstart() -> anyhow::Result<()> {
        let mut config = gaxi::options::ClientConfig::default();
        config.endpoint = Some("https://us-central1-aiplatform.googleapis.com".to_string());
        let client = Client::new(config).await?;

        let mut request = v1::ListModelsRequest::default();
        request.parent = "projects/dbolduc-test/locations/us-central1".to_string();
        let models = client
            .list_models(request, RequestOptions::default())
            .await?
            .into_inner();

        println!("models={models:?}");

        Ok(())
    }
}
