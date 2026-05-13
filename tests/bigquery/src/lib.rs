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

use anyhow::Result;
use futures::stream::StreamExt;
use google_cloud_bigquery_v2::client::{DatasetService, JobService, TableService};
use google_cloud_bigquery_v2::model::{
    Dataset, DatasetReference, Job, JobConfiguration, JobConfigurationQuery, JobReference, Table,
    TableFieldSchema, TableReference, TableSchema,
};
use google_cloud_gax::{error::rpc::Code, paginator::ItemPaginator};
use google_cloud_test_utils::runtime_config::project_id;
use rand::{RngExt, distr::Alphanumeric};

const INSTANCE_LABEL: &str = "rust-sdk-integration-test";

pub async fn dataset_admin() -> Result<()> {
    let project_id = project_id()?;
    let client = DatasetService::builder().with_tracing().build().await?;
    cleanup_stale_datasets(&client, &project_id).await?;

    let dataset_id = random_dataset_id();

    println!("CREATING DATASET WITH ID: {dataset_id}");

    let create = client
        .insert_dataset()
        .set_project_id(&project_id)
        .set_dataset(
            Dataset::new()
                .set_dataset_reference(DatasetReference::new().set_dataset_id(&dataset_id))
                .set_labels([(INSTANCE_LABEL, "true")]),
        )
        .send()
        .await?;
    println!("CREATE DATASET = {create:?}");

    assert!(create.dataset_reference.is_some(), "{create:?}");

    let list = client
        .list_datasets()
        .set_project_id(&project_id)
        .set_filter(format!("labels.{INSTANCE_LABEL}"))
        .by_item()
        .into_stream();
    let items = list.collect::<Vec<_>>().await;
    println!("LIST DATASET = {} entries", items.len());

    assert!(
        items
            .iter()
            .any(|v| v.as_ref().unwrap().id.contains(&dataset_id))
    );

    client
        .delete_dataset()
        .set_project_id(&project_id)
        .set_dataset_id(&dataset_id)
        .set_delete_contents(true)
        .send()
        .await?;
    println!("DELETE DATASET");

    Ok(())
}

async fn cleanup_stale_datasets(client: &DatasetService, project_id: &str) -> Result<()> {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let stale_deadline = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let stale_deadline = stale_deadline - Duration::from_secs(48 * 60 * 60);
    let stale_deadline = stale_deadline.as_millis() as i64;

    let list = client
        .list_datasets()
        .set_project_id(project_id)
        .set_filter(format!("labels.{INSTANCE_LABEL}"))
        .by_item()
        .into_stream();
    let datasets = list.collect::<Vec<_>>().await;

    let pending_all_datasets = datasets
        .iter()
        .filter_map(|v| match v {
            Ok(v) => {
                if let Some(dataset_id) = extract_dataset_id(project_id, &v.id) {
                    return Some(
                        client
                            .get_dataset()
                            .set_project_id(project_id)
                            .set_dataset_id(dataset_id)
                            .send(),
                    );
                }
                None
            }
            Err(_) => None,
        })
        .collect::<Vec<_>>();

    let stale_datasets = futures::future::join_all(pending_all_datasets)
        .await
        .into_iter()
        .filter_map(|r| match r {
            Ok(dataset) => Some(dataset),
            Err(e) if e.status().is_some_and(|s| s.code == Code::NotFound) => None,
            Err(_) => panic!("expected a successful get_dataset()"),
        })
        .filter_map(|dataset| {
            if dataset
                .labels
                .get(INSTANCE_LABEL)
                .is_some_and(|v| v == "true")
                && dataset.creation_time < stale_deadline
            {
                return Some(dataset);
            }
            None
        })
        .collect::<Vec<_>>();

    println!("found {} stale datasets", stale_datasets.len());

    let pending_deletion: Vec<_> = stale_datasets
        .into_iter()
        .filter_map(|ds| {
            if let Some(dataset_id) = extract_dataset_id(project_id, &ds.id) {
                return Some(
                    client
                        .delete_dataset()
                        .set_project_id(project_id)
                        .set_dataset_id(dataset_id)
                        .set_delete_contents(true)
                        .send(),
                );
            }
            None
        })
        .collect();

    futures::future::join_all(pending_deletion).await;

    Ok(())
}

fn random_dataset_id() -> String {
    let rand_suffix = random_id_suffix();
    format!("rust_bq_test_dataset_{rand_suffix}")
}

fn random_job_id() -> String {
    let rand_suffix = random_id_suffix();
    format!("rust_bq_test_job_{rand_suffix}")
}

fn random_id_suffix() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}

fn extract_dataset_id(project_id: &str, id: &str) -> Option<String> {
    id.strip_prefix(format!("{project_id}:").as_str())
        .map(|v| v.to_string())
}

pub async fn job_service() -> Result<()> {
    let project_id = project_id()?;
    let client = JobService::builder().with_tracing().build().await?;
    cleanup_stale_jobs(&client, &project_id).await?;

    let job_id = random_job_id();
    println!("CREATING JOB WITH ID: {job_id}");

    let query = "SELECT 1 as one";
    let job = client
        .insert_job()
        .set_project_id(&project_id)
        .set_job(
            Job::new()
                .set_job_reference(JobReference::new().set_job_id(&job_id))
                .set_configuration(
                    JobConfiguration::new()
                        .set_labels([(INSTANCE_LABEL, "true")])
                        .set_query(JobConfigurationQuery::new().set_query(query)),
                ),
        )
        .send()
        .await?;
    println!("CREATE JOB = {job:?}");

    assert!(job.job_reference.is_some(), "{job:?}");

    let list = client
        .list_jobs()
        .set_project_id(&project_id)
        .by_item()
        .into_stream();
    let items = list.collect::<Vec<_>>().await;
    println!("LIST JOBS = {} entries", items.len());

    assert!(
        items
            .iter()
            .any(|v| v.as_ref().unwrap().id.contains(&job_id))
    );

    Ok(())
}

async fn cleanup_stale_jobs(client: &JobService, project_id: &str) -> Result<()> {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let stale_deadline = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let stale_deadline = stale_deadline - Duration::from_secs(48 * 60 * 60);
    let stale_deadline = stale_deadline.as_millis() as u64;

    let list = client
        .list_jobs()
        .set_project_id(project_id)
        .set_max_creation_time(stale_deadline)
        .by_item()
        .into_stream();
    let items = list.collect::<Vec<_>>().await;
    println!("LIST JOBS = {} entries", items.len());

    let pending_all_stale_jobs = items
        .iter()
        .filter_map(|v| match v {
            Ok(v) => {
                if let Some(job_reference) = &v.job_reference {
                    return Some(
                        client
                            .get_job()
                            .set_project_id(project_id)
                            .set_job_id(&job_reference.job_id)
                            .send(),
                    );
                }
                None
            }
            Err(_) => None,
        })
        .collect::<Vec<_>>();

    let pending_deletion = futures::future::join_all(pending_all_stale_jobs)
        .await
        .into_iter()
        .filter_map(|r| match r {
            Ok(r) => {
                let job_reference = r.job_reference?;
                if r.configuration
                    .is_some_and(|c| c.labels.get(INSTANCE_LABEL).is_some_and(|v| v == "true"))
                    && r.status.is_some_and(|s| s.state == "DONE")
                {
                    return Some(
                        client
                            .delete_job()
                            .set_project_id(project_id)
                            .set_job_id(&job_reference.job_id)
                            .send(),
                    );
                }
                None
            }
            Err(_) => None,
        })
        .collect::<Vec<_>>();

    println!("found {} stale test jobs", pending_deletion.len());

    futures::future::join_all(pending_deletion).await;
    Ok(())
}

pub async fn writes() -> Result<()> {
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;

    let project_id = project_id()?;
    let dataset_service = DatasetService::builder().with_tracing().build().await?;
    let table_service = TableService::builder().with_tracing().build().await?;
    let write_client = google_cloud_bigquery_write::client::Client::builder()
        .build()
        .await?;

    cleanup_stale_datasets(&dataset_service, &project_id).await?;

    let dataset_id = random_dataset_id();
    println!("CREATING DATASET WITH ID: {dataset_id}");
    dataset_service
        .insert_dataset()
        .set_project_id(&project_id)
        .set_dataset(
            Dataset::new()
                .set_dataset_reference(DatasetReference::new().set_dataset_id(&dataset_id))
                .set_labels([(INSTANCE_LABEL, "true")]),
        )
        .send()
        .await?;
    println!("DATASET CREATED");

    let table_id = random_table_id();
    println!("CREATING TABLE WITH ID: {table_id}");
    let bq_schema = TableSchema::new().set_fields([
        TableFieldSchema::new().set_name("col1").set_type("STRING"),
        TableFieldSchema::new().set_name("col2").set_type("INTEGER"),
    ]);

    table_service
        .insert_table()
        .set_project_id(&project_id)
        .set_dataset_id(&dataset_id)
        .set_table(
            Table::new()
                .set_table_reference(
                    TableReference::new()
                        .set_project_id(&project_id)
                        .set_dataset_id(&dataset_id)
                        .set_table_id(&table_id),
                )
                .set_schema(bq_schema),
        )
        .send()
        .await?;
    println!("TABLE CREATED");

    // Create Arrow Schema
    let arrow_schema = Arc::new(Schema::new(vec![
        Field::new("col1", DataType::Utf8, false),
        Field::new("col2", DataType::Int64, false),
    ]));

    // Serialize Schema
    let mut schema_buf = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut schema_buf, &arrow_schema)?;
        writer.finish()?;
    }
    let schema_full_len = schema_buf.len();
    // StreamWriter writes [Schema Message] [End-of-stream]
    // End-of-stream is 0xFFFFFFFF 0x00000000 (8 bytes)
    if schema_buf.ends_with(&[255, 255, 255, 255, 0, 0, 0, 0]) {
        schema_buf.truncate(schema_buf.len() - 8);
    }
    println!("Schema message size: {}", schema_buf.len());

    // Create Arrow Record Batch
    let col1 = StringArray::from(vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
    let col2 = Int64Array::from(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    let batch = RecordBatch::try_new(arrow_schema.clone(), vec![Arc::new(col1), Arc::new(col2)])?;

    // Serialize Record Batch
    // We want to extract just the RecordBatch message.
    // The StreamWriter writes [Schema Message] [RecordBatch Message] [End-of-stream]
    let mut batch_buf = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut batch_buf, &arrow_schema)?;
        writer.write(&batch)?;
        writer.finish()?;
    }

    // The schema message length (without footer) is schema_full_len - 8.
    // However, the StreamWriter writes the schema message (without footer) at the start.
    // So the RecordBatch message starts at schema_full_len - 8.
    let schema_msg_len = schema_full_len - 8;
    let rb_msg = batch_buf[schema_msg_len..(batch_buf.len() - 8)].to_vec();
    println!("RecordBatch message size: {}", rb_msg.len());

    let stream_name =
        format!("projects/{project_id}/datasets/{dataset_id}/tables/{table_id}/streams/_default");

    let arrow_schema_proto =
        google_cloud_bigquery_write::google::cloud::bigquery::storage::v1::ArrowSchema {
            serialized_schema: schema_buf.into(),
        };

    let stream_writer = write_client.write_stream(stream_name, arrow_schema_proto);

    let arrow_batch_proto =
        google_cloud_bigquery_write::google::cloud::bigquery::storage::v1::ArrowRecordBatch {
            serialized_record_batch: rb_msg.into(),
            row_count: 10,
        };

    let _response = stream_writer.append(arrow_batch_proto).await?;
    drop(stream_writer);
    drop(write_client);

    // Verify
    let job_service = JobService::builder().build().await?;
    let query_config = JobConfigurationQuery::new()
        .set_query(format!(
            "SELECT * FROM `{project_id}.{dataset_id}.{table_id}`"
        ))
        .set_use_legacy_sql(false);

    let job = job_service
        .insert_job()
        .set_project_id(&project_id)
        .set_job(Job::new().set_configuration(JobConfiguration::new().set_query(query_config)))
        .send()
        .await?;

    let job_id = job.job_reference.as_ref().unwrap().job_id.clone();

    // Wait for job completion and get results
    let mut attempts = 0;
    let row_count = loop {
        let results = job_service
            .get_query_results()
            .set_project_id(&project_id)
            .set_job_id(&job_id)
            .send()
            .await?;

        if results.job_complete.unwrap_or(false) {
            break results.total_rows.unwrap_or(0);
        }

        attempts += 1;
        if attempts > 10 {
            anyhow::bail!("Query job did not complete in time");
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    };

    assert_eq!(row_count, 10);

    Ok(())
}

fn random_table_id() -> String {
    let rand_suffix = random_id_suffix();
    format!("rust_bq_test_table_{rand_suffix}")
}
