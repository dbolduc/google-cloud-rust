#!/bin/bash
set -euo pipefail

echo "Cloning the repo..."
git clone https://github.com/googleapis/google-cloud-rust.git --depth 1
cd google-cloud-rust

echo "Rerunning terraform for rust-auth-testing..."
cd src/auth/.gcb/builds
terraform init
terraform plan -out /tmp/auth.plan
terraform apply /tmp/auth.plan
