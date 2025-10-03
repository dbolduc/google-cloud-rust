# Use the official Ubuntu image from Docker Hub as
# a base image
FROM debian:bookworm-slim

# Execute next commands in the directory /workspace
WORKDIR /workspace

# Install git
RUN apt-get update && apt-get --no-install-recommends install -y git wget

# Install terraform
# https://developer.hashicorp.com/terraform/tutorials/aws-get-started/install-cli
RUN apt-get update && apt-get --no-install-recommends install -y gnupg software-properties-common
RUN wget -O- https://apt.releases.hashicorp.com/gpg | \
        gpg --dearmor | \
        tee /usr/share/keyrings/hashicorp-archive-keyring.gpg > /dev/null
RUN echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $(grep -oP '(?<=UBUNTU_CODENAME=).*' /etc/os-release || lsb_release -cs) main" | tee /etc/apt/sources.list.d/hashicorp.list
RUN apt update && apt-get --no-install-recommends install -y terraform

# Copy over the script to the /workspace directory
COPY script.sh .

# Just in case the script doesn't have the executable bit set
RUN chmod +x ./script.sh

# Run the script when starting the container
CMD [ "./script.sh" ]
