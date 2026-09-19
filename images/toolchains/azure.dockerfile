# azure toolchain: the Azure CLI, installed into its own venv so it stays out of
# the way of any project python.
ARG BASE=claude_here:base
FROM ${BASE}

RUN apt-get update \
 && apt-get install -y --no-install-recommends python3-dev libffi-dev \
 && rm -rf /var/lib/apt/lists/* \
 && python3 -m venv /opt/azure-cli \
 && /opt/azure-cli/bin/pip install --no-cache-dir --upgrade pip \
 && /opt/azure-cli/bin/pip install --no-cache-dir azure-cli \
 && ln -s /opt/azure-cli/bin/az /usr/local/bin/az \
 && az version --output tsv | head -1
