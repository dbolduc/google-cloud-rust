# Copyright 2025 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

variable "project" {
  type = string
}

data "google_project" "project" {
}

# This service account is created externally. It is used for all the builds.
data "google_service_account" "integration-test-runner" {
  account_id = "integration-test-runner"
}

# The service account will need to read tarballs uploaded by `gcloud submit`.
resource "google_storage_bucket_iam_member" "sa-can-read-build-tarballs" {
  bucket = "${var.project}_cloudbuild"
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${data.google_service_account.integration-test-runner.email}"
}

output "runner" {
  value = data.google_service_account.integration-test-runner.id
}

##########################################
# Service Account Integration Test Setup #
##########################################

# TODO(dbolduc) - I don't love the organization (resources / grants / etc.)
# I think we should split it into:
# - Common Setup (GCB triggers, links, integration-test-runner, cloud-build-bucket)
# - service_account creds
# - authorized_user creds
# - etc. for new types (e.g. SA impersonation, EA azure, EA aws) (If possible)

# The service account we will use as our principal for testing service account credentials.
resource "google_service_account" "test-sa-creds-principal" {
  account_id = "test-sa-creds"
  display_name = "A service account for testing service account credentials"
}

# Generate a key for the test service account credentials.
resource "google_service_account_key" "test-sa-creds-principal-key" {
  service_account_id = google_service_account.test-sa-creds-principal.name
}

# This secret stores the ADC json for the principal testing service account credentials.
resource "google_secret_manager_secret" "test-sa-creds-json-secret" {
  secret_id   = "test-sa-creds-json"
  replication {
    auto {}
  }
}

# Store the test service account key in secret manager.
resource "google_secret_manager_secret_version" "test-sa-creds-json-secret-version" {
  secret      = google_secret_manager_secret.test-sa-creds-json-secret.id
  secret_data = base64decode(google_service_account_key.test-sa-creds-principal-key.private_key)
}

# The "secret" that will be accessed by the principal testing service account
# credentials.
#
# Note that this is not really a "secret", in that we are not trying to hide its
# contents.
#
# In order to validate our credential types, we need a GCP resource we can set
# fine-grained ACL on. We have picked Secret Manager secrets for this purpose.
#
resource "google_secret_manager_secret" "test-sa-creds-secret" {
  secret_id = "test-sa-creds-secret"
  replication {
    auto {}
  }
}

# Add a value to the secret.
resource "google_secret_manager_secret_version" "test-sa-creds-secret-version" {
  secret      = google_secret_manager_secret.test-sa-creds-secret.id

  # We do not care that the value is public. We are just testing ACLs.
  secret_data = "service_account"
}

# Set up secret permissions for service account credentials.
resource "google_secret_manager_secret_iam_member" "test-creds-sa-secret-member" {
  project = "${var.project}"
  secret_id = google_secret_manager_secret.test-sa-creds-secret.id
  role = "roles/secretmanager.secretAccessor"
  member = "serviceAccount:${resource.google_service_account.test-sa-creds-principal.email}"
}
