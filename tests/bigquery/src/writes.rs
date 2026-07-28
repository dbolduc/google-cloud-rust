// Copyright 2026 Google LLC
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
use super::*;

pub async fn run_writes() -> Result<()> {
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
    //run_proto_writes(&project_id, &dataset_id).await?;

    Ok(())
}

pub async fn run_arrow_writes(project_id: &str, dataset_id: &str) -> Result<()> {
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use google_cloud_bigquery_write::client::Write;
    use google_cloud_bigquery_write::model::ArrowRecordBatch;
    use google_cloud_bigquery_write::model::ArrowSchema;
    use std::sync::Arc;

    let client = Write::builder().build().await?;
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

    let table =
        format!("projects/{project_id}/datasets/{dataset_id}/tables/{table_id}");
    let schema = ArrowSchema::new().set_serialized_schema(schema_buf);
    let writer = client.arrow(schema)
        .default(table);

    let rows = ArrowRecordBatch::new()
        .set_serialized_record_batch(rb_msg)
        .set_row_count(2);

    let response = writer.append(rows).send().await?;
    println!("Arrow rows appended: {response:?}");

    verify_test_table(project_id, dataset_id, &table_id).await?;
    println!("ARROW DATA CONTENT VERIFIED");

    anyhow::bail!("just want to see the output...");
    Ok(())
}

/*
pub async fn run_proto_writes(project_id: &str, dataset_id: &str) -> Result<()> {
    use google_cloud_bigquery_write::client::Write;
    use google_cloud_bigquery_write::model::ProtoRows;
    use google_cloud_bigquery_write::proto_schema::ProtoSchema;
    use google_cloud_wkt::{DescriptorProto, FieldDescriptorProto};
    use prost::Message;

    let write_client = Write::builder().build().await?;
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

    let stream_writer = write_client.write_stream_proto(stream_name, schema).await?;

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
        serialized_rows.push(prost::bytes::Bytes::from(buf));
    }

    let proto_rows = ProtoRows::new().set_serialized_rows(serialized_rows);

    let _response = stream_writer.append(proto_rows).await?;
    println!("Proto rows appended");

    verify_test_table(project_id, dataset_id, &table_id).await?;
    println!("PROTO DATA CONTENT VERIFIED");

    Ok(())
}
*/

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
