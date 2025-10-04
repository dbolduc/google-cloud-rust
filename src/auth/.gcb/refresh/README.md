# A Cloud Run job to rerun terraform

We want to periodically rerun terraform in order to:

1. rotate service account keys, which eventually expire
2. make sure our terraform configuration is up-to-date

## Background

We follow the instructions in: https://cloud.google.com/run/docs/quickstarts/jobs/build-create-shell

## Create terraform-runner SA

TODO : manage terraform-runner via Overground

Create SA:

```sh
gcloud --project=rust-auth-testing \
    iam service-accounts create "terraform-runner" \
    --display-name="Integration test resource updater" \
    --description="Service account for running Terraform automation."
```

Add roles - for now, admin. We will revoke later.

```sh
gcloud projects add-iam-policy-binding "rust-auth-testing" \
        --member="serviceAccount:terraform-runner@rust-auth-testing.iam.gserviceaccount.com" \
        --role="roles/admin" \
        --condition=None
```

Need the following permissions for GCE default SA to deploy:

```sh
gcloud projects add-iam-policy-binding "rust-auth-testing" \
    --member="serviceAccount:977362148892-compute@developer.gserviceaccount.com" \
    --role="roles/storage.objectViewer" \
    --condition=None
gcloud projects add-iam-policy-binding "rust-auth-testing" \
    --member="serviceAccount:977362148892-compute@developer.gserviceaccount.com" \
    --role="roles/logging.logWriter" \
    --condition=None

# This actually might be the only one we need.
gcloud projects add-iam-policy-binding "rust-auth-testing" \
    --member="serviceAccount:977362148892-compute@developer.gserviceaccount.com" \
    --role="roles/run.builder" \
    --condition=None
```

We need to be able to read the external project SA.

```sh
# TODO : add some sort of role to testsa@rust-external-account-joonix.iam.gserviceaccount.com?
# I started with "View serivce accounts"

# THat didn't work. I think I just need project level permissions to view SAs and WIF pools
# I just granted "Viewer" role on the project to terraform-runner
```

## Deploy

Enable the Cloud Run Admin API:

```sh
# TODO : I just clicked buttons in Pantheon 
```

Deploy the image:

```sh
gcloud run jobs deploy refresh \
    --project=rust-auth-testing \
    --source . \
    --tasks 1 \
    --max-retries 0 \
    --region us-central1 \
    --service-account=terraform-runner@rust-auth-testing.iam.gserviceaccount.com
```

The default of 512 MiB is not enough apparently. Ask for more memory:

```sh
gcloud run jobs update refresh \
    --project=rust-auth-testing \
    --region=us-central1 \
    --memory=2GiB
```

## Run

We want Cloud Scheduler to trigger this, but we can test it manually with:

```sh
gcloud run jobs execute refresh --project=rust-auth-testing
```

## Next steps

- Add SA with permissions to manage integration test resources.
  - Hm, the integration-test-runner might actually be good enough. That is worth trying.
- Run the function via Cloud Scheduler
- (stretch) have terraform own all of this configuration.
  - FWIW, I do not think this is so necessary.
  - And we should make incremental progress.
