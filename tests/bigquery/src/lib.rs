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
    let project_id = project_id()?;
    let dataset_service = DatasetService::builder().with_tracing().build().await?;
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

    run_arrow_writes(&project_id, &dataset_id).await?;
    run_proto_writes(&project_id, &dataset_id).await?;

    Ok(())
}

pub async fn run_arrow_writes(project_id: &str, dataset_id: &str) -> Result<()> {
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;

    let write_client = google_cloud_bigquery_write::client::Client::builder()
        .build()
        .await?;
    let table_service = TableService::builder().with_tracing().build().await?;

    let table_id = create_test_table(&table_service, project_id, dataset_id).await?;
    println!("ARROW TABLE CREATED: {table_id}");

    // Create Arrow Schema
    let arrow_schema = Arc::new(Schema::new(vec![
        Field::new("name", DataType::Utf8, false),
        Field::new("age", DataType::Int64, false),
    ]));

    let serialize_schema = |schema: &Schema| -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let _ = StreamWriter::try_new(&mut buf, schema)?;
        Ok(buf)
    };

    let serialize_batch = |batch: &RecordBatch| -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let schema_len = {
            let mut schema_buf = Vec::new();
            let _ = StreamWriter::try_new(&mut schema_buf, &batch.schema())?;
            schema_buf.len()
        };
        let mut writer = StreamWriter::try_new(&mut buf, &batch.schema())?;
        writer.write(batch)?;
        Ok(buf[schema_len..].to_vec())
    };

    // Serialize Schema
    let schema_buf = serialize_schema(&arrow_schema)?;

    // Create Arrow Record Batch
    let name = StringArray::from(vec!["Jim", "Jane"]);
    let age = Int64Array::from(vec![35, 27]);
    let batch = RecordBatch::try_new(arrow_schema.clone(), vec![Arc::new(name), Arc::new(age)])?;

    // Serialize Record Batch
    let rb_msg = serialize_batch(&batch)?;

    let stream_name =
        format!("projects/{project_id}/datasets/{dataset_id}/tables/{table_id}/streams/_default");

    let arrow_schema_proto =
        google_cloud_bigquery_write::google::cloud::bigquery::storage::v1::ArrowSchema {
            serialized_schema: schema_buf.into(),
        };

    let stream_writer = write_client.write_stream_arrow(stream_name, arrow_schema_proto);

    let arrow_batch_proto =
        google_cloud_bigquery_write::google::cloud::bigquery::storage::v1::ArrowRecordBatch {
            serialized_record_batch: rb_msg.into(),
            row_count: 2,
        };

    let _response = stream_writer.append(arrow_batch_proto).await?;
    println!("Arrow rows appended");

    verify_test_table(project_id, dataset_id, &table_id).await?;
    println!("ARROW DATA CONTENT VERIFIED");

    Ok(())
}

pub async fn run_proto_writes(project_id: &str, dataset_id: &str) -> Result<()> {
    use google_cloud_bigquery_write::google::cloud::bigquery::storage::v1::ProtoRows;
    use google_cloud_bigquery_write::proto_schema::ProtoSchema;
    use google_cloud_wkt::{DescriptorProto, FieldDescriptorProto};
    use prost::Message;

    let write_client = google_cloud_bigquery_write::client::Client::builder()
        .build()
        .await?;
    let table_service = TableService::builder().with_tracing().build().await?;

    let table_id = create_test_table(&table_service, project_id, dataset_id).await?;
    println!("PROTO TABLE CREATED: {table_id}");

    // Manually construct DescriptorProto
    let descriptor = DescriptorProto::new().set_name("SampleData").set_field([
        FieldDescriptorProto::new()
            .set_name("name")
            .set_number(1)
            .set_type(google_cloud_wkt::field_descriptor_proto::Type::String)
            .set_label(google_cloud_wkt::field_descriptor_proto::Label::Optional),
        FieldDescriptorProto::new()
            .set_name("age")
            .set_number(2)
            .set_type(google_cloud_wkt::field_descriptor_proto::Type::Int64)
            .set_label(google_cloud_wkt::field_descriptor_proto::Label::Optional),
    ]);

    let schema = ProtoSchema {
        proto_descriptor: Some(descriptor),
    };

    let stream_name =
        format!("projects/{project_id}/datasets/{dataset_id}/tables/{table_id}/streams/_default");

    let stream_writer = write_client.write_stream_proto(stream_name, schema)?;

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct SampleData {
        #[prost(string, tag = "1")]
        pub name: String,
        #[prost(int64, tag = "2")]
        pub age: i64,
    }

    let rows = vec![
        SampleData {
            name: "Jim".to_string(),
            age: 35,
        },
        SampleData {
            name: "Jane".to_string(),
            age: 27,
        },
    ];

    let mut serialized_rows = Vec::new();
    for row in rows {
        let mut buf = Vec::new();
        row.encode(&mut buf)?;
        serialized_rows.push(buf.into());
    }

    let proto_rows = ProtoRows { serialized_rows };

    let _response = stream_writer.append(proto_rows).await?;
    println!("Proto rows appended");

    verify_test_table(project_id, dataset_id, &table_id).await?;
    println!("PROTO DATA CONTENT VERIFIED");

    Ok(())
}

async fn create_test_table(
    table_service: &TableService,
    project_id: &str,
    dataset_id: &str,
) -> Result<String> {
    let table_id = random_table_id();
    let bq_schema = TableSchema::new().set_fields([
        TableFieldSchema::new().set_name("name").set_type("STRING"),
        TableFieldSchema::new().set_name("age").set_type("INTEGER"),
    ]);

    table_service
        .insert_table()
        .set_project_id(project_id)
        .set_dataset_id(dataset_id)
        .set_table(
            Table::new()
                .set_table_reference(
                    TableReference::new()
                        .set_project_id(project_id)
                        .set_dataset_id(dataset_id)
                        .set_table_id(&table_id),
                )
                .set_schema(bq_schema),
        )
        .send()
        .await?;

    Ok(table_id)
}

async fn verify_test_table(project_id: &str, dataset_id: &str, table_id: &str) -> Result<()> {
    let job_service = JobService::builder().build().await?;
    let query_config = JobConfigurationQuery::new()
        .set_query(format!(
            "SELECT * FROM `{project_id}.{dataset_id}.{table_id}` ORDER BY name"
        ))
        .set_use_legacy_sql(false);

    let job = job_service
        .insert_job()
        .set_project_id(project_id)
        .set_job(Job::new().set_configuration(JobConfiguration::new().set_query(query_config)))
        .send()
        .await?;

    let job_id = job.job_reference.as_ref().unwrap().job_id.clone();

    // Wait for job completion and get results
    let mut attempts = 0;
    let results = loop {
        let results = job_service
            .get_query_results()
            .set_project_id(project_id)
            .set_job_id(&job_id)
            .send()
            .await?;

        if results.job_complete.unwrap_or(false) {
            break results;
        }

        attempts += 1;
        if attempts > 10 {
            anyhow::bail!("Query job did not complete in time");
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    };

    assert_eq!(results.total_rows.unwrap_or(0), 2);

    // Verify content
    let rows = &results.rows;
    assert_eq!(rows.len(), 2);

    // Jane, 27 (ordered by name)
    let jane_row = rows[0].get("f").and_then(|f| f.as_array()).unwrap();
    assert_eq!(
        jane_row[0].get("v").and_then(|v| v.as_str()).unwrap(),
        "Jane"
    );
    assert_eq!(jane_row[1].get("v").and_then(|v| v.as_str()).unwrap(), "27");

    // Jim, 35
    let jim_row = rows[1].get("f").and_then(|f| f.as_array()).unwrap();
    assert_eq!(jim_row[0].get("v").and_then(|v| v.as_str()).unwrap(), "Jim");
    assert_eq!(jim_row[1].get("v").and_then(|v| v.as_str()).unwrap(), "35");

    Ok(())
}

fn random_table_id() -> String {
    let rand_suffix = random_id_suffix();
    format!("rust_bq_test_table_{rand_suffix}")
}
