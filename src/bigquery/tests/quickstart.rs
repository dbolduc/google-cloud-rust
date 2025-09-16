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
    use gax::options::RequestOptions;
    use google_cloud_bigquery::BQReadClient;
    use google_cloud_bigquery::google::cloud::bigquery::storage::v1;

    #[tokio::test]
    async fn quickstart() -> anyhow::Result<()> {
        let client = BQReadClient::new(gaxi::options::ClientConfig::default()).await?;

        // Create read session
        let mut request = v1::CreateReadSessionRequest::default();
        request.parent = "projects/dbolduc-test".to_string();
        request.read_session = {
            let mut read_session = v1::ReadSession::default();
            read_session.data_format = v1::DataFormat::Avro as i32;
            read_session.table =
                "projects/bigquery-public-data/datasets/usa_names/tables/usa_1910_current"
                    .to_string();
            Some(read_session)
        };
        request.max_stream_count = 1;
        let mut session = client
            .create_read_session(request, RequestOptions::default())
            .await?
            .into_inner();

        // Read rows - open stream
        let mut request = v1::ReadRowsRequest::default();
        request.read_stream = session.streams.remove(0).name;
        let mut stream = client
            .read_rows(request, RequestOptions::default())
            .await?
            .into_inner();

        // Read rows - drain stream
        let mut count = 0;
        while let Some(resp) = stream.message().await? {
            count += resp.row_count;
        }
        println!("Total: {count} rows.");

        Ok(())
    }
}
