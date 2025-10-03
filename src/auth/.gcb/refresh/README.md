# A Cloud Run job to rerun terraform

We want to periodically rerun terraform in order to:

1. rotate service account keys, which eventually expire
2. make sure our terraform configuration is up-to-date

## Background

We follow the instructions in: https://cloud.google.com/run/docs/quickstarts/jobs/build-create-shell

## Deploy

Enable the Cloud Run Admin API:

```sh
# TODO : I just clicked buttons in Pantheon 
```

Deploy the image:

```sh
gcloud run jobs deploy refresh \            
    --source . \
    --tasks 1 \
    --max-retries 1 \              
    --region us-central1 \             
    --project=dbolduc-test
```

I need this extra flag, because I deleted my default compute service accout a few years ago. Lol.

```sh
--service-account=compute-default@dbolduc-test.iam.gserviceaccount.com
```

TODO : I think in the long run, we want to have a SA that has the permissions
needed to create terraform resources. Typically we run terraform with our user
account (which has Owner permissions in the testing projects) as principal.

## Run

We want Cloud Scheduler to trigger this, but we can test it manually with:

```sh
gcloud run jobs execute refresh
```

## Next steps

- Add SA with permissions to manage integration test resources.
  - Hm, the integration-test-runner might actually be good enough. That is worth trying.
- Run the function via Cloud Scheduler
- (stretch) have terraform own all of this configuration.
  - FWIW, I do not think this is so necessary.
  - And we should make incremental progress.
